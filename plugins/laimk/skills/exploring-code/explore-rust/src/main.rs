//! explore-rust: the exploring-code skill's `explore`, rewritten in Rust with ripgrep's search crates and
//! tree-sitter grammars in place of the rg and ast-grep processes. Its output is the Python script's.

mod args;
mod defs;
mod hits;
mod json;
mod py;
mod search;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufWriter, StdoutLock, Write};
use std::os::unix::ffi::OsStringExt;

use defs::{Definitions, Parsed};

const HELP: &str = "\
Search a repository's code and read it by definition.

  explore-rust hits [RG_FLAGS] PATTERN [PATH...]   matches grouped under their enclosing definitions
  explore-rust outline FILE...                     each file's definitions with their line ranges
  explore-rust show FILE:LINE | FILE:START-END | FILE START [END] ...
                                                   numbered windows (FILE:LINE gives LINE-10..LINE+20)

hits runs two searches with ripgrep's own search crates: an rg walk that also
searches hidden files and skips only .git, and a search of the tracked files
that .gitignore hides from a plain rg, whose file lines are tagged
[tracked, hidden by .gitignore]. Binary files are skipped; submodules and
nested repositories are searched like any directory. A tracked file that a
submodule's own .gitignore hides is found only when hits runs inside that
submodule.

Each match is shown under its owner chain (impl Foo > fn new), with the
innermost definition's line range. Definitions come from a tree-sitter parse
of the file, with the node kinds ast-grep 0.51.3 reports; a file in a language
explore-rust does not parse shows (no parser), a file with no definitions
shows (no definitions found), a file the parser skips shows
(ast-grep skipped: REASON), with ast-grep's wording for the reason: no
language for an extension it does not know in that case, such as .JAVA,
emptyFile, fileTooLarge (over 3,000,000 bytes and 200,000 lines) or
permissionDenied; a file whose parse failed shows (ast-grep failed).

RG_FLAGS are rg's, before or after PATTERN; after -- every token is PATTERN or
PATH. Examples: -i -w -F -P -U -L -t java -g '*.rs' -C 2, and -E latin1, rg's
--encoding, for text that is not UTF-8. -n and -N are accepted and dropped. A
flag hits cannot honour is refused: rg's printing options, such as -o, -M,
--column or --trim, and flags that print something other than a search, such
as --files or --type-list. With -e PATTERN or -f FILE, every positional is a
PATH. -w refuses a PATTERN whose first or last character is a literal non-word
character, such as the . and ( of a pattern for .due(, which -w would match
only beside another non-word character. -l and -c print one line per file with
its count of matching lines, --count-matches with its count of matches. --max
N (or --max=N) caps the matches printed (default 300); over the cap, hits
prints counts by directory and file instead.

--json makes hits and outline print one JSON document, for a script to read;
show prints text only, for reading. Paths are as the text prints them, lines
1-based. A def is {start, end, sig, kind, name}: its line range, the line
holding its name, its ast-grep node kind (function_item, ...), and its name,
null when it has none (a Rust impl is named by its type). A skip is
{file, why, reason?}: a file that shows no definitions, with why one of
\"no parser\", \"no definitions\", \"failed\", \"skipped\" (reason: ast-grep's, or
\"no language\").
  hits:    {total, files, searched, cap, over, counts, rows, skips, hidden, errors}
           total and files as the header counts them; counts is {path: n} for
           every file when the text prints counts (-l, -c, --count-matches, or
           over the cap), else null. rows lists the matches when the text does,
           and is null when it prints counts instead: {file, line, text,
           owners: [def, outermost first], test, subs, hidden, ctx}, with text
           the whole line and ctx the -A/-B/-C lines grouped with it,
           [{line, text}]. test is a guess from names that the match is test
           code: a directory named test(s), testing, __tests__ or spec(s), a
           file named as tests are (test_*.py, *_test.go, *Test.java,
           *.spec.ts, ...), or an owner that is a module named tests or test or
           a class named Test*, *Test or *Tests; it cannot see a test module
           declared in another file, as #[cfg(test)] mod x; does. subs has one
           {col, end, in, def} per match on the line: col and end are 0-based
           character columns, the match being text[col:end] (under -U a match
           may run on into the lines below, its columns counting their
           newlines); in is where the match starts: \"code\", \"comment\" or
           \"string\", or null when ast-grep did not parse the file, or a warning
           says its lines are too long to place matches in; def is true when
           the match is the name of a definition this line holds. hidden lists
           the matched files tagged hidden, errors all of rg's errors.
  outline: {files: [{file, defs: [def + depth]}], skips, missing}
           missing lists the FILE arguments that are not files.
warnings (both) holds what text mode writes to stderr, so a --json run that
succeeds writes only the document and exits 0. An error exits nonzero with
its message on stderr and no document, as in text.";

/// Print MSG as explore's error and exit with status 2.
pub fn die(msg: &str) -> ! {
    eprintln!("explore: {msg}");
    std::process::exit(2)
}

/// Buffered stdout; a closed pipe ends the process quietly.
pub struct Out(BufWriter<StdoutLock<'static>>);

impl Out {
    /// DOC as one line of compact JSON.
    pub fn json<T: serde::Serialize>(&mut self, doc: &T) {
        if json::write(&mut self.0, doc).is_err() {
            std::process::exit(0);
        }
    }

    pub fn line(&mut self, s: &str) {
        if writeln!(self.0, "{s}").is_err() {
            std::process::exit(0);
        }
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Files' lines, decoded as UTF-8 or else Latin-1 and split on \n alone, read once each.
#[derive(Default)]
pub struct Lines(HashMap<Vec<u8>, Vec<String>>);

impl Lines {
    pub fn insert(&mut self, path: &[u8], text: &str) -> &Vec<String> {
        self.0.entry(path.to_vec()).or_insert_with(|| text.split('\n').map(str::to_string).collect())
    }

    pub fn read(&mut self, path: &[u8]) -> std::io::Result<&Vec<String>> {
        if !self.0.contains_key(path) {
            let data = std::fs::read(args::os(path))?;
            let text = py::decode(&data).into_owned();
            self.insert(path, &text);
        }
        Ok(&self.0[path])
    }
}

/// An OS error as Python's `strerror` gives it, without Rust's "(os error N)".
fn strerror(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.rfind(" (os error ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

fn is_file(path: &[u8]) -> bool {
    std::fs::metadata(args::os(path)).is_ok_and(|m| m.is_file())
}

/// Each definition with its depth: how many of the definitions before it hold it.
fn nested(defs: &[defs::Def]) -> Vec<(usize, &defs::Def)> {
    let mut stack: Vec<usize> = Vec::new();
    defs.iter()
        .map(|def| {
            while stack.last().is_some_and(|&end| end < def.start) {
                stack.pop();
            }
            let depth = stack.len();
            stack.push(def.end);
            (depth, def)
        })
        .collect()
}

fn outline(files: Vec<OsString>, out: &mut Out) {
    let as_json = files.iter().any(|f| f == "--json");
    let files: Vec<Vec<u8>> = files.into_iter().filter(|f| f != "--json").map(OsString::into_vec).collect();
    let missing = files.iter().any(|f| !is_file(f));
    let (mut lines, mut defs) = (Lines::default(), Definitions::new());
    let present: Vec<Vec<u8>> = files.iter().filter(|f| is_file(f)).map(|f| py::normpath(f)).collect();
    defs.parse_all(&present);
    if as_json {
        for path in &present {
            defs.get(path, &mut lines);
        }
        let parsed: Vec<(&[u8], Option<&Parsed>, Vec<(usize, &defs::Def)>)> = present
            .iter()
            .map(|path| {
                let parsed = defs.files[path].as_ref();
                let found = match parsed {
                    Some(Parsed::Defs(d)) => nested(d),
                    _ => Vec::new(),
                };
                (path.as_slice(), parsed, found)
            })
            .collect();
        let absent: Vec<Vec<u8>> = files.iter().filter(|f| !is_file(f)).cloned().collect();
        // the document names the missing files, so the run succeeds
        return out.json(&json::outline(&parsed, &absent));
    }
    for f in files.iter().filter(|f| !is_file(f)) {
        out.line(&format!("{}: no such file", py::display(f)));
    }
    for path in &present {
        let shown = py::display(path);
        match defs.get(path, &mut lines) {
            None => out.line(&format!("{shown}: (no parser)")),
            Some(Parsed::Skipped(why)) => out.line(&format!("{shown}: ({why})")),
            Some(Parsed::Defs(d)) if d.is_empty() => out.line(&format!("{shown}: (no definitions found)")),
            Some(Parsed::Defs(d)) => {
                out.line(&shown);
                for (depth, def) in nested(d) {
                    let pad = "  ".repeat(depth);
                    out.line(&format!("  {pad}{}-{}  {}", def.start, def.end, py::slice(&def.sig, 0, 150)));
                }
            }
        }
    }
    out.flush().ok();
    if missing {
        std::process::exit(1);
    }
}

/// FILE:LINE, FILE:START-END, FILE:START,END or FILE:LINE:COL: Python's `^(.*?):(\d+)(?:([-,:])(\d+))?$`, whose
/// `$` also matches before a final newline.
fn parse_spec(spec: &[u8]) -> Option<(Vec<u8>, u64, Option<(u8, u64)>)> {
    let digits = |s: &[u8]| s.iter().take_while(|b| b.is_ascii_digit()).count();
    let at_end = |s: &[u8]| s.is_empty() || s == b"\n";
    for (i, _) in spec.iter().enumerate().filter(|(_, b)| **b == b':') {
        if spec[..i].contains(&b'\n') {
            return None;
        }
        let rest = &spec[i + 1..];
        let n = digits(rest);
        if n == 0 {
            continue;
        }
        let line = py::int(&rest[..n]);
        let tail = &rest[n..];
        if at_end(tail) {
            return Some((spec[..i].to_vec(), line, None));
        }
        if b"-,:".contains(&tail[0]) {
            let m = digits(&tail[1..]);
            if m > 0 && at_end(&tail[1 + m..]) {
                return Some((spec[..i].to_vec(), line, Some((tail[0], py::int(&tail[1..1 + m])))));
            }
        }
    }
    None
}

fn show(argv: Vec<OsString>, out: &mut Out) {
    if argv.iter().any(|a| a == "--json") {
        die("show prints text only; --json is for hits and outline");
    }
    let mut rest: Vec<Vec<u8>> = argv.into_iter().map(OsString::into_vec).collect();
    let mut windows: Vec<(Vec<u8>, u64, u64)> = Vec::new();
    while !rest.is_empty() {
        if rest.len() >= 2 && !rest[0].contains(&b':') && py::is_digits(&rest[1]) {
            let start = py::int(&rest[1]);
            let has_end = rest.len() > 2 && py::is_digits(&rest[2]);
            let end = if has_end { py::int(&rest[2]) } else { start.saturating_add(59) };
            windows.push((rest[0].clone(), start, end));
            rest.drain(..if has_end { 3 } else { 2 });
            continue;
        }
        let spec = rest.remove(0);
        let Some((path, line, second)) = parse_spec(&spec) else {
            die(&format!("show takes FILE:LINE, FILE:START-END or FILE START [END], not {}", py_repr(&spec)));
        };
        match second {
            Some((sep, b)) if sep != b':' || b >= line => windows.push((path, line, b)),
            // FILE:LINE, or FILE:LINE:COL as compilers print it
            _ => windows.push((path, line.saturating_sub(10).max(1), line.saturating_add(20))),
        }
    }
    if windows.is_empty() {
        die("show takes FILE:LINE, FILE:START-END or FILE START [END]");
    }
    let mut lines = Lines::default();
    let mut failed = false;
    let many = windows.len() > 1;
    for (path, start, end) in &windows {
        let shown = py::display(path);
        let file = match lines.read(path) {
            Ok(l) => l,
            Err(e) => {
                out.line(&format!("{shown}: {}", strerror(&e)));
                failed = true;
                continue;
            }
        };
        // a final newline ends the last line rather than starting one
        let count = (file.len() - (file.last().is_some_and(String::is_empty) as usize)) as u64;
        let (first, last) = ((*start).max(1), (*end).min(count));
        if first > last {
            out.line(&format!("{shown} has {}", py::plural(count as usize, "line")));
            continue;
        }
        if many {
            out.line(&format!("== {shown}:{start}-{end}"));
        }
        for n in first..=last {
            out.line(&format!("{n:>6}  {}", file[(n - 1) as usize]));
        }
    }
    out.flush().ok();
    if failed {
        std::process::exit(1);
    }
}

/// Python's `repr` of a command-line string, for error messages.
fn py_repr(s: &[u8]) -> String {
    let text = py::display(s);
    if text.contains('\'') && !text.contains('"') {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

fn main() {
    let mut argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut out = Out(BufWriter::new(std::io::stdout().lock()));
    // -h or --help anywhere before a -- asks for this help, whichever command it follows
    let before_dashes = argv.iter().take_while(|a| *a != "--");
    if argv.is_empty() || before_dashes.into_iter().any(|a| a == "-h" || a == "--help") {
        out.line(HELP);
        out.flush().ok();
        return;
    }
    let command = argv.remove(0);
    match command.to_str() {
        Some("hits") => hits::hits(argv, &mut out),
        Some("outline") => outline(argv, &mut out),
        Some("show") => show(argv, &mut out),
        _ => die(&format!("unknown command {}; see explore --help", py_repr(command.as_encoded_bytes()))),
    }
    out.flush().ok();
}
