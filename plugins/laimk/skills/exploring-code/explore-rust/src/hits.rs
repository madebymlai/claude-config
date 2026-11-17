//! `hits`: the search's matches grouped under their enclosing definitions, or counts when they pass the cap. The
//! search and the grouping give plain values, and printing them is a step of its own.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;

use crate::defs::{Def, Definitions, Parsed};
use crate::search::{Found, Row, search};
use crate::{Out, die, json, py};

const TAG: &str = "  [tracked, hidden by .gitignore]";

/// Python's `Counter.most_common`: by count, highest first, ties in insertion order.
fn most_common(counts: &[(Vec<u8>, u64)]) -> Vec<&(Vec<u8>, u64)> {
    let mut v: Vec<&(Vec<u8>, u64)> = counts.iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

pub fn hits(argv: Vec<OsString>, out: &mut Out) {
    let args = crate::args::parse_hits(argv);
    let Found { per_file, rows, searched, errors, hidden } = search(&args);
    let tag = |p: &[u8]| if hidden.contains(p) { TAG } else { "" };
    let total: u64 = per_file.values().sum();
    if total == 0 {
        if !errors.is_empty() {
            die(&errors[..errors.len().min(5)].join("\n"));
        }
        if searched == 0 && hidden.is_empty() {
            die("searched 0 files; check the PATH arguments and the working directory");
        }
    }
    let mut files: Vec<(Vec<u8>, u64)> = per_file.into_iter().collect();
    files.sort_by_cached_key(|(p, _)| py::sort_key(p));
    if args.json {
        return out.json(&document(&args, total, files, rows, searched, &errors, &hidden));
    }
    if total == 0 {
        return out.line(&format!("0 matches in {} searched", py::plural(searched as usize, "file")));
    }
    if args.digest.is_some() {
        for (path, n) in &files {
            out.line(&format!("{n:>6}  {}{}", py::display(path), tag(path)));
        }
    } else if total > args.cap {
        let mut per_dir: Vec<(Vec<u8>, u64)> = Vec::new();
        let mut index: HashMap<Vec<u8>, usize> = HashMap::new();
        for (path, n) in &files {
            let d = py::dirname(path);
            let d = if d.is_empty() { b".".to_vec() } else { d.to_vec() };
            let i = *index.entry(d.clone()).or_insert_with(|| {
                per_dir.push((d, 0));
                per_dir.len() - 1
            });
            per_dir[i].1 += n;
        }
        out.line(&format!(
            "{} in {}, over the cap of {}: counts below. Narrow with PATH, -t or -g, or raise --max.",
            py::plural(total as usize, "match"),
            py::plural(files.len(), "file"),
            args.cap
        ));
        out.line("by directory:");
        for (d, n) in most_common(&per_dir).into_iter().take(20) {
            out.line(&format!("  {n:>6}  {}/", py::display(d)));
        }
        out.line("by file:");
        for (p, n) in most_common(&files).into_iter().take(20) {
            out.line(&format!("  {n:>6}  {}{}", py::display(p), tag(p)));
        }
    } else {
        print_grouped(&group(&listed(rows, false)), &tag, out);
    }
    if !errors.is_empty() {
        out.line(&format!(
            "rg reported {}; the results above cover the other files:",
            py::plural(errors.len(), "error")
        ));
        for e in errors.iter().take(5) {
            out.line(&format!("  {e}"));
        }
    }
}

/// The JSON document of a search: the rows and skips when the text lists matches, the counts when it prints counts.
fn document<'a>(
    args: &crate::args::Hits,
    total: u64,
    files: Vec<(Vec<u8>, u64)>,
    rows: HashMap<Vec<u8>, HashSet<Row>>,
    searched: u64,
    errors: &'a [String],
    hidden: &HashSet<Vec<u8>>,
) -> json::HitsOwned<'a> {
    let listed_files = total > 0 && args.digest.is_none() && total <= args.cap;
    let over = args.digest.is_none() && total > args.cap;
    let hidden_files = files.iter().filter(|(p, _)| hidden.contains(p)).map(|(p, _)| json::Name::new(p)).collect();
    let (total, count, counts, listing) = if listed_files {
        let listing = listed(rows, true);
        let matched = listing.iter().filter(|f| f.rows.iter().any(|r| r.is_match)).count();
        let matches = listing.iter().map(|f| f.rows.iter().filter(|r| r.is_match).count() as u64).sum();
        (matches, matched, None, listing)
    } else {
        let counts = (total > 0).then(|| json::Counts(files.iter().map(|(p, n)| (json::Name::new(p), *n)).collect()));
        (total, files.len(), counts, Vec::new())
    };
    json::HitsOwned {
        total,
        files: count,
        searched,
        cap: args.cap,
        over,
        counts,
        listing,
        hidden: hidden_files,
        tagged: hidden.clone(),
        errors,
    }
}

/// Where a match sits: in a definition, outermost owner first, or a label for a file without one.
#[derive(Clone, PartialEq)]
enum Heading {
    Label(&'static str),
    Skipped(String),
    Def { start: usize, end: usize, owners: Vec<String> },
}

impl Heading {
    fn of(parsed: Option<&Parsed>, line: u64) -> Heading {
        let defs: &[Def] = match parsed {
            None => return Heading::Label("no parser"),
            Some(Parsed::Skipped(why)) => return Heading::Skipped(why.clone()),
            Some(Parsed::Defs(d)) if d.is_empty() => return Heading::Label("no definitions found"),
            Some(Parsed::Defs(d)) => d,
        };
        let chain = owner_chain(defs, line);
        match chain.last() {
            None => Heading::Label("top level"),
            Some(inner) => {
                Heading::Def { start: inner.start, end: inner.end, owners: chain.iter().map(|d| d.sig.clone()).collect() }
            }
        }
    }

    fn label(&self) -> Option<String> {
        match self {
            Heading::Label(l) => Some(l.to_string()),
            Heading::Skipped(why) => Some(why.clone()),
            Heading::Def { .. } => None,
        }
    }

    fn text(&self) -> String {
        match self {
            Heading::Def { start, end, owners } => {
                let short = |s: &str| {
                    let s = py::strip(s.trim_end_matches(['{', ' ']));
                    if py::char_len(s) > 90 { format!("{}...", py::slice(s, 0, 87)) } else { s.to_string() }
                };
                let names: Vec<String> = owners.iter().map(|s| short(s)).collect();
                format!("{start}-{end} {}", names.join(" > "))
            }
            other => format!("({})", other.label().expect("not a definition")),
        }
    }
}

/// A file's rows in line order, each with the line of the match it belongs to, and the file's parse: what both the
/// text and the JSON are made from.
pub struct Listed {
    pub path: Vec<u8>,
    pub rows: Vec<Row>,
    /// a match's own line, and for a context line its nearest match's, the later one when two are as near
    pub anchors: Vec<u64>,
    /// None for a file with no parser
    pub parsed: Option<Parsed>,
    /// its comments and strings, when they were looked for and it parsed
    pub places: Option<Vec<crate::defs::Place>>,
}

/// The rows of each file sorted, anchored and parsed, files in Python's order; with PLACED, each parsed file's
/// comments and strings too.
fn listed(rows: HashMap<Vec<u8>, HashSet<Row>>, placed: bool) -> Vec<Listed> {
    let mut files: Vec<(Vec<u8>, Vec<Row>)> = rows
        .into_iter()
        .map(|(p, set)| {
            let mut v: Vec<Row> = set.into_iter().collect();
            v.sort_by_key(|r| r.line);
            (p, v)
        })
        .collect();
    files.sort_by_cached_key(|(p, _)| py::sort_key(p));
    let mut defs = if placed { Definitions::with_places() } else { Definitions::new() };
    let matched: Vec<Vec<u8>> =
        files.iter().filter(|(_, rs)| rs.iter().any(|r| r.is_match)).map(|(p, _)| p.clone()).collect();
    defs.parse_all(&matched);
    files
        .into_iter()
        .map(|(path, rows)| {
            let match_lines: Vec<u64> = rows.iter().filter(|r| r.is_match).map(|r| r.line).collect();
            let anchors = rows
                .iter()
                .map(|r| {
                    if r.is_match {
                        r.line
                    } else {
                        *match_lines.iter().min_by_key(|&&m| (m.abs_diff(r.line), m < r.line)).expect("context has a match")
                    }
                })
                .collect();
            let parsed = defs.files.remove(&path).flatten();
            let places = defs.places.remove(&path).filter(|_| matches!(parsed, Some(Parsed::Defs(_))));
            Listed { path, rows, anchors, parsed, places }
        })
        .collect()
}

/// The definitions holding LINE, outermost first; a definition starting on LINE is the match itself.
pub fn owner_chain(defs: &[Def], line: u64) -> Vec<&Def> {
    defs.iter().filter(|d| (d.start as u64) < line && line <= d.end as u64).collect()
}

/// A file's rows, in groups that share a heading, in line order.
struct FileGroups<'a> {
    path: &'a [u8],
    groups: Vec<(Heading, Vec<&'a Row>)>,
}

fn group(files: &[Listed]) -> Vec<FileGroups<'_>> {
    files
        .iter()
        .map(|f| {
            let mut heads: HashMap<u64, Heading> = HashMap::new();
            let mut groups: Vec<(Heading, Vec<&Row>)> = Vec::new();
            for (r, &anchor) in f.rows.iter().zip(&f.anchors) {
                let head = heads.entry(anchor).or_insert_with(|| Heading::of(f.parsed.as_ref(), anchor)).clone();
                match groups.iter_mut().find(|(h, _)| *h == head) {
                    Some((_, g)) => g.push(r),
                    None => groups.push((head, vec![r])),
                }
            }
            FileGroups { path: &f.path, groups }
        })
        .collect()
}

fn counts(files: &[FileGroups]) -> (usize, usize, usize) {
    let rows = files.iter().flat_map(|f| &f.groups).flat_map(|(_, g)| g);
    let matches = rows.clone().filter(|r| r.is_match).count();
    let matched_files =
        files.iter().filter(|f| f.groups.iter().flat_map(|(_, g)| g).any(|r| r.is_match)).count();
    (matches, rows.count(), matched_files)
}

/// WIDTH characters of TEXT around COLUMN, with ... where the line is cut.
fn clip(text: &str, column: i64, width: i64) -> String {
    let len = py::char_len(text) as i64;
    if len <= width {
        return text.to_string();
    }
    let start = 0.max((column - width / 4).min(len - width));
    let body = py::slice(text, start as usize, (start + width) as usize);
    format!("{}{body}{}", if start > 0 { "..." } else { "" }, if start + width < len { "..." } else { "" })
}

/// The longest common prefix of STRINGS, by characters, as `os.path.commonprefix`.
fn common_prefix_len<'a>(strings: impl Iterator<Item = &'a str>) -> usize {
    let mut prefix: Option<Vec<char>> = None;
    for s in strings {
        match &mut prefix {
            None => prefix = Some(s.chars().collect()),
            Some(p) => {
                let n = p.iter().zip(s.chars()).take_while(|(a, b)| **a == *b).count();
                p.truncate(n);
            }
        }
    }
    prefix.map_or(0, |p| p.len())
}

fn print_grouped(files: &[FileGroups], tag: &dyn Fn(&[u8]) -> &'static str, out: &mut Out) {
    let (matches, rows, matched_files) = counts(files);
    // with context lines a group keeps its relative indentation, so a line shows which block it sits in
    let context = matches < rows;
    out.line(&format!("{} in {}", py::plural(matches, "match"), py::plural(matched_files, "file")));
    for f in files {
        out.line(&format!("{}{}", py::display(f.path), tag(f.path)));
        for (head, group) in &f.groups {
            out.line(&format!("  {}", head.text()));
            let indent = if context {
                common_prefix_len(group.iter().filter(|r| !py::strip(&r.text).is_empty()).map(|r| py::indent(&r.text)))
            } else {
                0
            };
            for r in group {
                let cut = if context { indent } else { py::char_len(py::indent(&r.text)) };
                let body = py::slice(py::rstrip(&r.text), cut, usize::MAX);
                let mark = if r.is_match { ':' } else { '-' };
                out.line(&format!("  {:>6}{mark}   {}", r.line, clip(body, r.column as i64 - cut as i64, 160)));
            }
        }
    }
}
