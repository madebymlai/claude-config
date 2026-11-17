//! `--json` for `hits` and `outline`: the documents the Python script prints, field for field and byte for byte.
//! Fields are declared in its key order, and serde_json's compact writer escapes as `json.dumps(ensure_ascii=False)`
//! does.

use serde::Serialize;
use serde::ser::SerializeMap;

use std::collections::{HashMap, HashSet};

use crate::defs::{Def, Parsed, Place};
use crate::hits::{Listed, owner_chain};
use crate::py;

/// A file name as the Python script's JSON holds it: as its text output prints it, each undecodable byte as the
/// six characters `\udcXX`.
pub struct Name(String);

impl Name {
    pub fn new(name: &[u8]) -> Name {
        Name(py::display(name))
    }
}

impl Serialize for Name {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

/// DOC as one line of JSON on W.
pub fn write<T: Serialize, W: std::io::Write>(w: &mut W, doc: &T) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, doc).map_err(std::io::Error::other)?;
    w.write_all(b"\n")
}

/// A definition: `Def._asdict()`.
#[derive(Serialize)]
pub struct JsonDef<'a> {
    start: usize,
    end: usize,
    sig: &'a str,
    kind: &'a str,
    name: Option<&'a str>,
}

impl<'a> From<&'a Def> for JsonDef<'a> {
    fn from(d: &'a Def) -> Self {
        JsonDef { start: d.start, end: d.end, sig: &d.sig, kind: &d.kind, name: d.name.as_deref() }
    }
}

/// A definition in an outline, with how many definitions hold it.
#[derive(Serialize)]
pub struct OutlineDef<'a> {
    #[serde(flatten)]
    def: JsonDef<'a>,
    depth: usize,
}

/// A file that shows no definitions, and why.
#[derive(Serialize)]
pub struct Skip {
    file: Name,
    why: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// The skip entry of a file PARSED shows no definitions for, if it shows none.
pub fn skip(file: &[u8], parsed: Option<&Parsed>) -> Option<Skip> {
    let (why, reason) = match parsed {
        Some(Parsed::Defs(d)) if !d.is_empty() => return None,
        None => ("no parser", None),
        Some(Parsed::Defs(_)) => ("no definitions", None),
        Some(Parsed::Skipped(s)) => match s.strip_prefix("ast-grep skipped: ") {
            Some(reason) => ("skipped", Some(reason.to_string())),
            None => ("failed", None),
        },
    };
    Some(Skip { file: Name::new(file), why, reason })
}

#[derive(Serialize)]
pub struct Ctx<'a> {
    pub line: u64,
    pub text: &'a str,
}

/// A match: its line, whole, its owner chain, and the context lines that go with it.
#[derive(Serialize)]
pub struct Row<'a> {
    pub file: Name,
    pub line: u64,
    pub text: &'a str,
    pub owners: Vec<JsonDef<'a>>,
    pub test: bool,
    pub subs: Vec<Sub>,
    pub hidden: bool,
    pub ctx: Vec<Ctx<'a>>,
}

/// A submatch: its character columns, whether it starts in code, a comment or a string, and whether it is the name
/// of a definition its line holds.
#[derive(Serialize)]
pub struct Sub {
    col: usize,
    end: usize,
    #[serde(rename = "in")]
    place: Option<&'static str>,
    def: bool,
}

/// Whether a file named BASE is named as tests are: test.rs, tests.rs, testing.rs, test_*.py, conftest.py,
/// *_test.(go|py|rs|rb|ex|exs), *Test(s)/*IT.(java|kt|scala), *.(test|spec).[jt]sx?, *_spec.rb.
fn test_file(base: &str) -> bool {
    if matches!(base, "test.rs" | "tests.rs" | "testing.rs" | "conftest.py")
        || base.starts_with("test_") && base.ends_with(".py")
    {
        return true;
    }
    let Some((stem, ext)) = base.rsplit_once('.') else { return false };
    match ext {
        "go" | "py" | "rs" | "ex" | "exs" => stem.ends_with("_test"),
        "rb" => stem.ends_with("_test") || stem.ends_with("_spec"),
        "java" | "kt" | "scala" => stem.ends_with("Test") || stem.ends_with("Tests") || stem.ends_with("IT"),
        "js" | "jsx" | "ts" | "tsx" => stem.ends_with(".test") || stem.ends_with(".spec"),
        _ => false,
    }
}

/// Whether a match in PATH, inside the definitions CHAIN, looks like test code: a directory or file named as tests
/// are, a module named tests or test, or a class named Test*, *Test or *Tests. A guess from names alone, blind to a
/// module that a #[cfg(test)] declares in another file.
fn is_test(path_bytes: &[u8], chain: &[&Def]) -> bool {
    let path = String::from_utf8_lossy(path_bytes);
    let mut parts: Vec<&str> = path.split('/').collect();
    let base = parts.pop().unwrap_or("");
    if parts.iter().any(|d| matches!(*d, "test" | "tests" | "testing" | "__tests__" | "spec" | "specs")) || test_file(base) {
        return true;
    }
    chain.iter().any(|d| match d.name.as_deref() {
        Some(n) if d.kind == "mod_item" => n == "tests" || n == "test",
        Some(n) if d.kind.contains("class") => {
            n.strip_prefix("Test").and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_uppercase() || c == '_')
                || n.ends_with("Test")
                || n.ends_with("Tests")
        }
        _ => false,
    }) || test_module(path_bytes)
}

thread_local! {
    static TEST_MODULES: std::cell::RefCell<HashMap<Vec<u8>, bool>> = Default::default();
}

/// Python's `test_module`: whether PATH is a Rust file that its parent module's file declares under #[cfg(test)], as
/// #[cfg(test)] mod x; does, or whose parent module is such a file.
fn test_module(path: &[u8]) -> bool {
    if let Some(known) = TEST_MODULES.with(|c| c.borrow().get(path).copied()) {
        return known;
    }
    TEST_MODULES.with(|c| c.borrow_mut().insert(path.to_vec(), false));
    let split = |p: &[u8]| -> (Vec<u8>, Vec<u8>) {
        match p.iter().rposition(|&b| b == b'/') {
            Some(i) => (p[..i].to_vec(), p[i + 1..].to_vec()),
            None => (Vec::new(), p.to_vec()),
        }
    };
    let (mut dir, base) = split(path);
    let mut name = match base.strip_suffix(b".rs") {
        Some(stem) if base != b"lib.rs" && base != b"main.rs" => stem.to_vec(),
        _ => Vec::new(),
    };
    if name == b"mod" {
        (dir, name) = split(&dir);
    }
    let mut result = false;
    if !name.is_empty() {
        let join = |f: &str| if dir.is_empty() { f.as_bytes().to_vec() } else { [&dir[..], b"/", f.as_bytes()].concat() };
        let mut parents = vec![join("mod.rs"), join("lib.rs"), join("main.rs")];
        if !dir.is_empty() {
            parents.push([&dir[..], b".rs"].concat());
        }
        let name = String::from_utf8_lossy(&name).into_owned();
        for parent in parents {
            let Ok(data) = std::fs::read(crate::args::os(&parent)) else { continue };
            let text = py::decode(&data);
            if declares(&text, &name, true) {
                result = true;
                break;
            }
            if declares(&text, &name, false) {
                result = test_module(&parent);
                break;
            }
        }
    }
    TEST_MODULES.with(|c| c.borrow_mut().insert(path.to_vec(), result));
    result
}

/// Whether TEXT declares module NAME by `mod NAME;`, with GATED under #[cfg(test)] and any other attributes and a
/// pub between, as Python's regexes read it.
fn declares(text: &str, name: &str, gated: bool) -> bool {
    let ws = |s: &str| s.len() - s.trim_start_matches(char::is_whitespace).len();
    // mod, whitespace, NAME, whitespace, ;
    let mod_decl = |s: &str| -> bool {
        let Some(r) = s.strip_prefix("mod") else { return false };
        let k = ws(r);
        if k == 0 {
            return false;
        }
        let Some(r) = r[k..].strip_prefix(name) else { return false };
        r[ws(r)..].starts_with(';')
    };
    if !gated {
        return text.match_indices("mod").any(|(i, _)| {
            !text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') && mod_decl(&text[i..])
        });
    }
    text.match_indices("#[cfg(test)]").any(|(i, m)| {
        let mut r = &text[i + m.len()..];
        r = &r[ws(r)..];
        while r.starts_with("#[") {
            let Some(j) = r.find(']') else { return false };
            r = &r[j + 1..];
            r = &r[ws(r)..];
        }
        if let Some(after) = r.strip_prefix("pub") {
            let mut a = after;
            if a.starts_with('(') {
                match a.find(')') {
                    Some(j) => a = &a[j + 1..],
                    None => return mod_decl(r),
                }
            }
            let k = ws(a);
            if k > 0 && mod_decl(&a[k..]) {
                return true;
            }
        }
        mod_decl(r)
    })
}

/// Whether the match at COL to END on line LINE, whose text is TEXT, is the name of a definition in DEFS that the
/// line holds.
fn is_def(defs: &[Def], line: u64, text: &str, col: usize, end: usize) -> bool {
    defs.iter().any(|d| {
        d.start as u64 <= line
            && line <= d.end as u64
            && d.sig == py::strip(text)
            && d.name.as_deref() == Some(py::slice(text, col, end))
    })
}

/// Where a character at 0-based LINE and COL sits among PLACES: in the innermost that holds it, the one starting
/// last, else in code.
fn place(places: &[Place], line: usize, col: usize) -> &'static str {
    places
        .iter()
        .filter(|p| p.start <= (line, col) && (line, col) < p.end)
        // the first of equals, as Python's max keeps
        .fold(None::<&Place>, |best, p| match best {
            Some(b) if (b.start, std::cmp::Reverse(b.end)) >= (p.start, std::cmp::Reverse(p.end)) => Some(b),
            _ => Some(p),
        })
        .map_or("code", |p| p.what)
}

/// Per-file counts, as a JSON object in the files' order.
pub struct Counts(pub Vec<(Name, u64)>);

impl Serialize for Counts {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct Hits<'a> {
    total: u64,
    files: usize,
    searched: u64,
    cap: u64,
    over: bool,
    counts: &'a Option<Counts>,
    rows: Option<Vec<Row<'a>>>,
    skips: Vec<Skip>,
    hidden: &'a [Name],
    errors: &'a [String],
    warnings: Warnings,
}

/// What the Python script's --json holds in place of its stderr warnings: that ast-grep is missing or exited
/// nonzero. explore-rust parses in process, so it has none to give.
#[derive(Serialize)]
struct Warnings([(); 0]);

/// What a `hits` document is made from; its rows and skips are drawn from LISTING when it is written.
pub struct HitsOwned<'a> {
    pub total: u64,
    pub files: usize,
    pub searched: u64,
    pub cap: u64,
    pub over: bool,
    pub counts: Option<Counts>,
    /// the files whose matches the text lists, empty when it prints counts
    pub listing: Vec<Listed>,
    pub hidden: Vec<Name>,
    /// the tracked files .gitignore hides
    pub tagged: HashSet<Vec<u8>>,
    pub errors: &'a [String],
}

impl Serialize for HitsOwned<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut rows = Vec::new();
        for f in &self.listing {
            let defs = match &f.parsed {
                Some(Parsed::Defs(d)) => d.as_slice(),
                _ => &[],
            };
            let mut at: HashMap<u64, usize> = HashMap::new();
            for r in f.rows.iter().filter(|r| r.is_match) {
                at.insert(r.line, rows.len());
                let chain = owner_chain(defs, r.line);
                rows.push(Row {
                    file: Name::new(&f.path),
                    line: r.line,
                    text: &r.text,
                    test: is_test(&f.path, &chain),
                    owners: chain.into_iter().map(JsonDef::from).collect(),
                    subs: r
                        .subs
                        .iter()
                        .map(|s| Sub {
                            col: s.col,
                            end: s.end,
                            place: f.places.as_deref().map(|p| place(p, r.line as usize - 1 + s.below, s.at)),
                            def: s.below == 0 && is_def(defs, r.line, &r.text, s.col, s.end),
                        })
                        .collect(),
                    hidden: self.tagged.contains(&f.path),
                    ctx: Vec::new(),
                });
            }
            for (r, anchor) in f.rows.iter().zip(&f.anchors).filter(|(r, _)| !r.is_match) {
                rows[at[anchor]].ctx.push(Ctx { line: r.line, text: &r.text });
            }
        }
        let skips = self
            .listing
            .iter()
            .filter(|f| f.rows.iter().any(|r| r.is_match))
            .filter_map(|f| skip(&f.path, f.parsed.as_ref()))
            .collect();
        Hits {
            total: self.total,
            files: self.files,
            searched: self.searched,
            cap: self.cap,
            over: self.over,
            counts: &self.counts,
            // null when the text prints counts instead of the matches
            rows: self.counts.is_none().then_some(rows),
            skips,
            hidden: &self.hidden,
            errors: self.errors,
            warnings: Warnings([]),
        }
        .serialize(s)
    }
}

#[derive(Serialize)]
pub struct OutlineFile<'a> {
    file: Name,
    defs: Vec<OutlineDef<'a>>,
}

#[derive(Serialize)]
pub struct Outline<'a> {
    files: Vec<OutlineFile<'a>>,
    skips: Vec<Skip>,
    /// the FILE arguments that are not files, as given
    missing: Vec<Name>,
    warnings: Warnings,
}

/// The outline of FILES, each with its parse and its definitions' depths, and of MISSING, the arguments that are
/// not files.
pub fn outline<'a>(files: &[(&[u8], Option<&'a Parsed>, Vec<(usize, &'a Def)>)], missing: &[Vec<u8>]) -> Outline<'a> {
    Outline {
        files: files
            .iter()
            .map(|(path, _, defs)| OutlineFile {
                file: Name::new(path),
                defs: defs.iter().map(|&(depth, d)| OutlineDef { def: d.into(), depth }).collect(),
            })
            .collect(),
        skips: files.iter().filter_map(|(path, parsed, _)| skip(path, *parsed)).collect(),
        missing: missing.iter().map(|f| Name::new(f)).collect(),
        warnings: Warnings([]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json<T: Serialize>(doc: &T) -> String {
        let mut out = Vec::new();
        write(&mut out, doc).unwrap();
        String::from_utf8(out).unwrap()
    }

    // expected values are CPython's: json.dumps(separators=(",", ":"), ensure_ascii=False), then the script's
    // SURROGATE.sub over the result
    #[test]
    fn undecodable_bytes_are_written_as_the_text_prints_them() {
        assert_eq!(json(&Name::new(b"lat/n\xe4me.rs")), "\"lat/n\\\\udce4me.rs\"\n");
    }

    #[test]
    fn text_escapes_as_python_json_does() {
        assert_eq!(json(&"a\"b\\c\td\r\x01\x7f\u{e9}\u{2028}"), "\"a\\\"b\\\\c\\td\\r\\u0001\x7f\u{e9}\u{2028}\"\n");
    }
}
