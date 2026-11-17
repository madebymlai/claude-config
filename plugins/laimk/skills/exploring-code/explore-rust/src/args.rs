//! `hits` arguments, in two stages as in Python's explore: its own parse, which picks out the cap, the patterns,
//! the paths and the digest and passes the rest on as rg flags, then rg's reading of those flags, which here builds
//! a [`Search`] instead of running rg.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use crate::die;

const SHORT_WITH_VALUE: &[u8] = b"ABCmMtTgefjdE";
const LONG_WITH_VALUE: &[&str] = &[
    "--type", "--type-not", "--type-add", "--type-clear", "--glob", "--iglob", "--after-context",
    "--before-context", "--context", "--max-count", "--max-columns", "--max-depth", "--max-filesize", "--encoding",
    "--engine", "--file", "--threads", "--dfa-size-limit", "--regex-size-limit", "--path-separator", "--pre",
    "--pre-glob", "--context-separator", "--ignore-file", "--color", "--colors", "--sort", "--sortr",
    "--hyperlink-format", "--field-context-separator", "--field-match-separator",
];
const DROPPED: &[&str] = &[
    "-n", "-N", "-R", "-H", "--with-filename", "--extended-regexp", "--line-number", "--no-line-number",
    "--recursive", "--color", "--colors", "--sort", "--sortr", "--sort-files", "--no-sort-files",
];
const DIGEST: &[&str] = &["-l", "-c", "--files-with-matches", "--count", "--count-matches"];
/// rg's flags that hits cannot honour: those of its standard printer and their negations, which leave its JSON as it
/// is, those that replace the search with other output, and those that only write to stderr
const REFUSED: &[&str] = &[
    "--files", "--files-without-match", "--vimgrep", "-q", "--quiet", "-r", "--replace", "--passthru", "--passthrough",
    "--stats", "--null", "-0", "--heading", "-p", "--pretty", "-b", "--byte-offset", "--no-byte-offset", "--column",
    "--no-column", "--context-separator", "--no-context-separator", "--field-context-separator",
    "--field-match-separator", "--hostname-bin", "--hyperlink-format", "--include-zero", "--no-include-zero", "-M",
    "--max-columns", "--max-columns-preview", "--no-max-columns-preview", "-o", "--only-matching", "--path-separator",
    "--trim", "--no-trim", "-I", "--no-filename", "--no-heading", "--no-stats", "--block-buffered",
    "--no-block-buffered", "--line-buffered", "--no-line-buffered", "-h", "--help", "-V", "--version",
    "--pcre2-version", "--type-list", "--generate", "--no-json", "--debug", "--trace",
];

/// What `-l`, `-c` or `--count-matches` prints per file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    MatchedLines,
    Matches,
}

pub struct Hits {
    pub cap: u64,
    /// the rg flags, as Python's explore would pass them to rg
    pub flags: Vec<Vec<u8>>,
    pub patterns: Vec<Vec<u8>>,
    pub paths: Vec<Vec<u8>>,
    pub digest: Option<Digest>,
    /// print the JSON document instead of the text
    pub json: bool,
}

fn is(tok: &[u8], set: &[&str]) -> bool {
    set.iter().any(|s| s.as_bytes() == tok)
}

/// Python's `parse_hits_args`.
pub fn parse_hits(argv: Vec<OsString>) -> Hits {
    let (mut cap, mut flags, mut positional, mut patterns) = (300u64, Vec::new(), Vec::new(), Vec::new());
    let (mut literal, mut from_file, mut json) = (false, false, false);
    let mut it = argv.into_iter().map(OsString::into_vec);
    let missing = || -> Vec<u8> { die("a flag is missing its value") };
    while let Some(tok) = it.next() {
        let (name, value) = match tok.iter().position(|&b| b == b'=') {
            Some(i) => (tok[..i].to_vec(), Some(tok[i + 1..].to_vec())),
            None => (tok.clone(), None),
        };
        if literal || !tok.starts_with(b"-") || tok == b"-" {
            positional.push(tok);
        } else if tok == b"--" {
            literal = true;
        } else if tok == b"--json" {
            json = true;

        } else if name == b"--max" {
            let value = value.unwrap_or_else(|| it.next().unwrap_or_else(missing));
            if !crate::py::is_digits(&value) {
                die("--max takes a number");
            }
            cap = crate::py::int(&value);
        } else if name == b"--regexp" {
            patterns.push(value.unwrap_or_else(|| it.next().unwrap_or_else(missing)));
        } else if name == b"--file" {
            flags.push(b"--file".to_vec());
            flags.push(value.unwrap_or_else(|| it.next().unwrap_or_else(missing)));
            from_file = true;
        } else if is(&name, REFUSED) {
            die(&format!("{} is not supported by hits; run rg directly for it", crate::py::display(&name)));
        } else if tok.starts_with(b"--") {
            let takes = is(&name, LONG_WITH_VALUE) && value.is_none();
            let value = if takes { Some(it.next().unwrap_or_else(missing)) } else { None };
            if !is(&name, DROPPED) {
                flags.push(tok);
                flags.extend(value);
            }
        } else {
            // a cluster such as -inw, whose last flag may take a value: -A2, -ie PAT; split by characters as Python
            // does, so a character that is not ASCII stays whole and an undecodable byte is one character
            let mut chars = Vec::new();
            for chunk in tok[1..].utf8_chunks() {
                chars.extend(chunk.valid().chars().map(|c| (c.to_string(), c.len_utf8())));
                chars.extend(chunk.invalid().iter().map(|&b| (crate::py::display(&[b]), 1)));
            }
            let mut offset = 1;
            for (ch, len) in chars {
                let flag = format!("-{ch}");
                offset += len;
                if is(flag.as_bytes(), REFUSED) {
                    die(&format!("{flag} is not supported by hits; run rg directly for it"));
                }
                if is(flag.as_bytes(), DROPPED) {
                    continue;
                }
                let ch = if ch.len() == 1 { ch.as_bytes()[0] as char } else { '\0' };
                if ch != '\0' && SHORT_WITH_VALUE.contains(&(ch as u8)) {
                    let rest = tok[offset.min(tok.len())..].to_vec();
                    let value = if rest.is_empty() { it.next().unwrap_or_else(missing) } else { rest };
                    if ch == 'e' {
                        patterns.push(value);
                    } else {
                        flags.push(flag.into_bytes());
                        flags.push(value);
                        from_file |= ch == 'f';
                    }
                    break;
                }
                flags.push(flag.into_bytes());
            }
        }
    }
    // as in rg: with no -e or -f the first positional is the pattern, with either every positional is a path
    if patterns.is_empty() && !from_file {
        if positional.is_empty() {
            die("usage: explore hits [RG_FLAGS] PATTERN [PATH...]   (a PATTERN starting with '-' goes after -e)");
        }
        patterns.push(positional.remove(0));
    }
    for p in &positional {
        if std::fs::metadata(std::ffi::OsStr::from_bytes(p)).is_err() {
            die(&format!("no such path: {}", crate::py::display(p)));
        }
    }
    let digest = if flags.iter().any(|f| is(f, DIGEST)) {
        Some(if flags.iter().any(|f| f == b"--count-matches") { Digest::Matches } else { Digest::MatchedLines })
    } else {
        None
    };
    flags.retain(|f| !is(f, DIGEST));
    Hits { cap, flags, patterns, paths: positional, digest, json }
}

// ---- rg's reading of the flags ----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
    Smart,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    Word,
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Default,
    Pcre2,
    Auto,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Binary {
    Auto,
    SearchAndSuppress,
    AsText,
}

#[derive(Clone)]
pub enum Encoding {
    Auto,
    Disabled,
    Label(String),
}

#[derive(Clone)]
pub enum TypeChange {
    Clear(String),
    Add(String),
    Select(String),
    Negate(String),
}

/// One rg search's settings: the subset of rg's arguments explore-rust implements.
#[derive(Clone)]
pub struct Search {
    pub case: Case,
    pub boundary: Option<Boundary>,
    pub fixed_strings: bool,
    pub multiline: bool,
    pub multiline_dotall: bool,
    pub crlf: bool,
    pub no_unicode: bool,
    pub engine: Engine,
    pub invert: bool,
    pub binary: Binary,
    pub encoding: Encoding,
    pub before: Option<usize>,
    pub after: Option<usize>,
    pub both: Option<usize>,
    pub max_count: Option<u64>,
    pub follow: bool,
    pub hidden: bool,
    pub max_depth: Option<usize>,
    pub max_filesize: Option<u64>,
    pub one_file_system: bool,
    pub no_ignore_dot: bool,
    pub no_ignore_exclude: bool,
    pub no_ignore_global: bool,
    pub no_ignore_parent: bool,
    pub no_ignore_vcs: bool,
    pub no_require_git: bool,
    pub unrestricted: u8,
    pub globs: Vec<String>,
    pub iglobs: Vec<String>,
    pub glob_case_insensitive: bool,
    pub types: Vec<TypeChange>,
    pub threads: Option<usize>,
    pub pattern_files: Vec<PathBuf>,
    pub null_data: bool,
    pub stop_on_nonmatch: bool,
    pub mmap: Mmap,
    pub regex_size_limit: Option<usize>,
    pub dfa_size_limit: Option<usize>,
    pub ignore_files: Vec<PathBuf>,
    pub no_ignore_files: bool,
    pub ignore_file_case_insensitive: bool,
    /// rg's error messages, and those about ignore files, are printed
    pub messages: bool,
    pub ignore_messages: bool,
    /// a command whose output is searched in place of each file's contents, for the files PRE_GLOBS takes
    pub pre: Option<PathBuf>,
    pub pre_globs: Vec<String>,
    pub search_zip: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mmap {
    Auto,
    Always,
    Never,
}

impl Search {
    pub fn context(&self) -> (usize, usize) {
        let (mut before, mut after) = self.both.map_or((0, 0), |n| (n, n));
        // --before and --after always override --context, wherever they appear
        if let Some(n) = self.before {
            before = n;
        }
        if let Some(n) = self.after {
            after = n;
        }
        (before, after)
    }
}

enum Kind {
    Switch(fn(&mut Search)),
    Value(fn(&mut Search, String) -> Result<(), String>),
}

fn number<T: std::str::FromStr>(v: &str) -> Result<T, String> {
    v.parse().map_err(|_| format!("invalid number: {v}"))
}

/// rg's sizes, `NUM[K|M|G]`.
fn filesize(v: &str) -> Result<u64, String> {
    grep_cli::parse_human_readable_size(v).map_err(|e| format!("invalid size: {e}"))
}

fn size(v: &str) -> Result<usize, String> {
    usize::try_from(filesize(v)?).map_err(|_| "size is too big".to_string())
}

fn no_ignore(s: &mut Search, yes: bool) {
    s.no_ignore_dot = yes;
    s.no_ignore_exclude = yes;
    s.no_ignore_global = yes;
    s.no_ignore_parent = yes;
    s.no_ignore_vcs = yes;
}

/// The rg flags explore-rust implements, by short and long name.
fn flag(name: &str) -> Option<Kind> {
    use Kind::*;
    Some(match name {
        "-i" | "--ignore-case" => Switch(|s| s.case = Case::Insensitive),
        "-S" | "--smart-case" => Switch(|s| s.case = Case::Smart),
        "-s" | "--case-sensitive" => Switch(|s| s.case = Case::Sensitive),
        "-w" | "--word-regexp" => Switch(|s| s.boundary = Some(Boundary::Word)),
        "-x" | "--line-regexp" => Switch(|s| s.boundary = Some(Boundary::Line)),
        "-F" | "--fixed-strings" => Switch(|s| s.fixed_strings = true),
        "--no-fixed-strings" => Switch(|s| s.fixed_strings = false),
        "-U" | "--multiline" => Switch(|s| s.multiline = true),
        "--no-multiline" => Switch(|s| s.multiline = false),
        "--multiline-dotall" => Switch(|s| s.multiline_dotall = true),
        "--no-multiline-dotall" => Switch(|s| s.multiline_dotall = false),
        "--crlf" => Switch(|s| s.crlf = true),
        "--no-crlf" => Switch(|s| s.crlf = false),
        "--no-unicode" => Switch(|s| s.no_unicode = true),
        "--unicode" => Switch(|s| s.no_unicode = false),
        "-P" | "--pcre2" => Switch(|s| s.engine = Engine::Pcre2),
        "--no-pcre2" => Switch(|s| s.engine = Engine::Default),
        "--engine" => Value(|s, v| {
            s.engine = match v.as_str() {
                "default" => Engine::Default,
                "pcre2" => Engine::Pcre2,
                "auto" => Engine::Auto,
                _ => return Err(format!("unrecognized regex engine '{v}'")),
            };
            Ok(())
        }),
        "-v" | "--invert-match" => Switch(|s| s.invert = true),
        "--no-invert-match" => Switch(|s| s.invert = false),
        "-a" | "--text" => Switch(|s| s.binary = Binary::AsText),
        "--no-text" | "--no-binary" => Switch(|s| s.binary = Binary::Auto),
        "--binary" => Switch(|s| s.binary = Binary::SearchAndSuppress),
        "-E" | "--encoding" => Value(|s, v| {
            s.encoding = match v.as_str() {
                "auto" => Encoding::Auto,
                "none" => Encoding::Disabled,
                _ => {
                    grep_searcher::Encoding::new(&v).map_err(|e| e.to_string())?;
                    Encoding::Label(v)
                }
            };
            Ok(())
        }),
        "--no-encoding" => Switch(|s| s.encoding = Encoding::Auto),
        "-A" | "--after-context" => Value(|s, v| Ok(s.after = Some(number(&v)?))),
        "-B" | "--before-context" => Value(|s, v| Ok(s.before = Some(number(&v)?))),
        "-C" | "--context" => Value(|s, v| Ok(s.both = Some(number(&v)?))),
        "-m" | "--max-count" => Value(|s, v| Ok(s.max_count = Some(number(&v)?))),
        "-L" | "--follow" => Switch(|s| s.follow = true),
        "--no-follow" => Switch(|s| s.follow = false),
        "-." | "--hidden" => Switch(|s| s.hidden = true),
        "--no-hidden" => Switch(|s| s.hidden = false),
        "-d" | "--max-depth" | "--maxdepth" => Value(|s, v| Ok(s.max_depth = Some(number(&v)?))),
        "--max-filesize" => Value(|s, v| Ok(s.max_filesize = Some(filesize(&v)?))),
        "--one-file-system" => Switch(|s| s.one_file_system = true),
        "--no-one-file-system" => Switch(|s| s.one_file_system = false),
        "--no-ignore" => Switch(|s| no_ignore(s, true)),
        "--ignore" => Switch(|s| no_ignore(s, false)),
        "--no-ignore-dot" => Switch(|s| s.no_ignore_dot = true),
        "--ignore-dot" => Switch(|s| s.no_ignore_dot = false),
        "--no-ignore-exclude" => Switch(|s| s.no_ignore_exclude = true),
        "--ignore-exclude" => Switch(|s| s.no_ignore_exclude = false),
        "--no-ignore-global" => Switch(|s| s.no_ignore_global = true),
        "--ignore-global" => Switch(|s| s.no_ignore_global = false),
        "--no-ignore-parent" => Switch(|s| s.no_ignore_parent = true),
        "--ignore-parent" => Switch(|s| s.no_ignore_parent = false),
        "--no-ignore-vcs" => Switch(|s| s.no_ignore_vcs = true),
        "--ignore-vcs" => Switch(|s| s.no_ignore_vcs = false),
        "--no-require-git" => Switch(|s| s.no_require_git = true),
        "--require-git" => Switch(|s| s.no_require_git = false),
        "-u" | "--unrestricted" => Switch(|s| {
            s.unrestricted = s.unrestricted.saturating_add(1);
            match s.unrestricted {
                1 => no_ignore(s, true),
                2 => s.hidden = true,
                _ => s.binary = Binary::AsText,
            }
        }),
        "-g" | "--glob" => Value(|s, v| Ok(s.globs.push(v))),
        "--iglob" => Value(|s, v| Ok(s.iglobs.push(v))),
        "--glob-case-insensitive" => Switch(|s| s.glob_case_insensitive = true),
        "--no-glob-case-insensitive" => Switch(|s| s.glob_case_insensitive = false),
        "-t" | "--type" => Value(|s, v| Ok(s.types.push(TypeChange::Select(v)))),
        "-T" | "--type-not" => Value(|s, v| Ok(s.types.push(TypeChange::Negate(v)))),
        "--type-add" => Value(|s, v| Ok(s.types.push(TypeChange::Add(v)))),
        "--type-clear" => Value(|s, v| Ok(s.types.push(TypeChange::Clear(v)))),
        "-j" | "--threads" => Value(|s, v| Ok(s.threads = Some(number(&v)?))),
        "-f" | "--file" => Value(|s, v| Ok(s.pattern_files.push(PathBuf::from(v)))),
        "--null-data" => Switch(|s| {
            s.crlf = false;
            s.null_data = true;
        }),
        "--stop-on-nonmatch" => Switch(|s| {
            s.stop_on_nonmatch = true;
            s.multiline = false;
        }),
        "--mmap" => Switch(|s| s.mmap = Mmap::Always),
        "--no-mmap" => Switch(|s| s.mmap = Mmap::Never),
        "--regex-size-limit" => Value(|s, v| Ok(s.regex_size_limit = Some(size(&v)?))),
        "--dfa-size-limit" => Value(|s, v| Ok(s.dfa_size_limit = Some(size(&v)?))),
        "--ignore-file" => Value(|s, v| Ok(s.ignore_files.push(PathBuf::from(v)))),
        "--no-ignore-files" => Switch(|s| s.no_ignore_files = true),
        "--ignore-files" => Switch(|s| s.no_ignore_files = false),
        "--ignore-file-case-insensitive" => Switch(|s| s.ignore_file_case_insensitive = true),
        "--no-ignore-file-case-insensitive" => Switch(|s| s.ignore_file_case_insensitive = false),
        "--no-messages" => Switch(|s| s.messages = false),
        "--messages" => Switch(|s| s.messages = true),
        "--no-ignore-messages" => Switch(|s| s.ignore_messages = false),
        "--ignore-messages" => Switch(|s| s.ignore_messages = true),
        "--auto-hybrid-regex" => Switch(|s| s.engine = Engine::Auto),
        "--no-auto-hybrid-regex" => Switch(|s| s.engine = Engine::Default),
        "--no-pcre2-unicode" => Switch(|s| s.no_unicode = true),
        "--pcre2-unicode" => Switch(|s| s.no_unicode = false),
        // rg already runs without a config file
        "--no-config" => Switch(|_| {}),
        "--pre" => Value(|s, v| {
            s.pre = if v.is_empty() { None } else { Some(PathBuf::from(v)) };
            if s.pre.is_some() {
                s.search_zip = false;
            }
            Ok(())
        }),
        "--no-pre" => Switch(|s| s.pre = None),
        "--pre-glob" => Value(|s, v| Ok(s.pre_globs.push(v))),
        "-z" | "--search-zip" => Switch(|s| {
            s.pre = None;
            s.search_zip = true;
        }),
        "--no-search-zip" => Switch(|s| s.search_zip = false),
        _ => return None,
    })
}

/// rg's reading of FLAGS. A flag explore-rust does not implement is refused rather than ignored.
pub fn rg_flags(flags: &[Vec<u8>]) -> Search {
    let mut s = Search {
        case: Case::Sensitive,
        boundary: None,
        fixed_strings: false,
        multiline: false,
        multiline_dotall: false,
        crlf: false,
        no_unicode: false,
        engine: Engine::Default,
        invert: false,
        binary: Binary::Auto,
        encoding: Encoding::Auto,
        before: None,
        after: None,
        both: None,
        max_count: None,
        follow: false,
        hidden: false,
        max_depth: None,
        max_filesize: None,
        one_file_system: false,
        no_ignore_dot: false,
        no_ignore_exclude: false,
        no_ignore_global: false,
        no_ignore_parent: false,
        no_ignore_vcs: false,
        no_require_git: false,
        unrestricted: 0,
        globs: Vec::new(),
        iglobs: Vec::new(),
        glob_case_insensitive: false,
        types: Vec::new(),
        threads: None,
        pattern_files: Vec::new(),
        null_data: false,
        stop_on_nonmatch: false,
        mmap: Mmap::Auto,
        regex_size_limit: None,
        dfa_size_limit: None,
        ignore_files: Vec::new(),
        no_ignore_files: false,
        ignore_file_case_insensitive: false,
        messages: true,
        ignore_messages: true,
        pre: None,
        pre_globs: Vec::new(),
        search_zip: false,
    };
    let mut it = flags.iter();
    while let Some(tok) = it.next() {
        let tok = String::from_utf8_lossy(tok);
        let (name, inline) = match tok.split_once('=') {
            Some((n, v)) if tok.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (tok.to_string(), None),
        };
        match flag(&name) {
            None => die(&format!("{name} is not supported by explore-rust")),
            Some(Kind::Switch(set)) => {
                if inline.is_some() {
                    die(&format!("rg: {name} does not take a value"));
                }
                set(&mut s);
            }
            Some(Kind::Value(set)) => {
                let value = match inline {
                    Some(v) => v,
                    None => match it.next() {
                        Some(v) => String::from_utf8(v.clone())
                            .unwrap_or_else(|_| die(&format!("rg: the value of {name} is not valid UTF-8"))),
                        None => die(&format!("rg: missing value for flag {name}")),
                    },
                };
                if let Err(e) = set(&mut s, value) {
                    die(&format!("rg: error parsing flag {name}: {e}"));
                }
            }
        }
    }
    s
}

/// A path argument as bytes.
pub fn os(p: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::from_bytes(p)
}
