//! `--json` for `hits` and `outline`: the documents the Python script prints, field for field and byte for byte.
//! Fields are declared in its key order, and serde_json's compact writer escapes as `json.dumps(ensure_ascii=False)`
//! does.

use serde::Serialize;
use serde::ser::SerializeMap;

use std::collections::{HashMap, HashSet};

use crate::defs::{Def, Parsed};
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
    pub hidden: bool,
    pub ctx: Vec<Ctx<'a>>,
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
    rows: Vec<Row<'a>>,
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
                rows.push(Row {
                    file: Name::new(&f.path),
                    line: r.line,
                    text: &r.text,
                    owners: owner_chain(defs, r.line).into_iter().map(JsonDef::from).collect(),
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
            rows,
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
