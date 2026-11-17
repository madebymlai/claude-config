//! The two searches `hits` runs, done as rg 15.2 does them with the crates rg is built from: an rg walk that also
//! searches hidden files and skips only .git, and a search of the tracked files .gitignore hides.
//!
//! Each search keeps what Python's explore reads from rg's JSON output: per file the count of matched lines or of
//! matches, and the match and context lines until more than the cap of matches has been seen.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use grep_matcher::Matcher;
use grep_searcher::{BinaryDetection, MmapChoice, Searcher, SearcherBuilder, Sink, SinkContext, SinkFinish, SinkMatch};
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::args::{Binary, Boundary, Case, Digest, Encoding, Engine, Hits, Mmap, Search, TypeChange, os, rg_flags};
use crate::{die, py};

/// grep-printer's bound on how far past a multi-line match its matches are looked for
const MAX_LOOK_AHEAD: usize = 128;

/// One line of rg's output: a match or a context line, its first line only, as Python's explore keeps it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Row {
    pub line: u64,
    pub text: String,
    pub is_match: bool,
    /// the character column of the line's first match
    pub column: usize,
    /// every match on a match row: its submatches
    pub subs: Vec<Sub>,
}

/// A submatch, in characters of the text rg reports for its row, which under -U runs on into the lines below.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sub {
    pub col: usize,
    pub end: usize,
    /// how many lines below the row's line it starts, and its column in that line
    pub below: usize,
    pub at: usize,
}

pub struct Found {
    /// per file, the count -l, -c or --count-matches prints, for each file where it is not zero
    pub per_file: HashMap<Vec<u8>, u64>,
    /// the distinct rows, by file
    pub rows: HashMap<Vec<u8>, HashSet<Row>>,
    pub searched: u64,
    pub errors: Vec<String>,
    /// the tracked files .gitignore hides, as Python's explore names them
    pub hidden: HashSet<Vec<u8>>,
}

/// The display form of a path rg reports: relative to the working directory however it was given, normalised.
pub fn display_path(path: &Path, cwd: &[u8]) -> Vec<u8> {
    let raw = path.as_os_str().as_bytes();
    if raw.starts_with(b"/") { py::normpath(&py::relpath(raw, cwd)) } else { py::normpath(raw) }
}

enum AnyMatcher {
    Rust(grep_regex::RegexMatcher),
    Pcre2(grep_pcre2::RegexMatcher),
}

fn matcher_rust(s: &Search, patterns: &[String]) -> Result<grep_regex::RegexMatcher, String> {
    let mut b = grep_regex::RegexMatcherBuilder::new();
    b.multi_line(true).unicode(!s.no_unicode).octal(false).fixed_strings(s.fixed_strings);
    match s.case {
        Case::Sensitive => b.case_insensitive(false),
        Case::Insensitive => b.case_insensitive(true),
        Case::Smart => b.case_smart(true),
    };
    match s.boundary {
        Some(Boundary::Line) => {
            b.whole_line(true);
        }
        Some(Boundary::Word) => {
            b.word(true);
        }
        None => {}
    }
    if s.multiline {
        b.dot_matches_new_line(s.multiline_dotall);
        if s.crlf {
            b.crlf(true).line_terminator(None);
        }
    } else {
        b.line_terminator(Some(b'\n')).dot_matches_new_line(false);
        if s.crlf {
            b.crlf(true);
        }
        if s.null_data {
            b.line_terminator(Some(b'\x00'));
        }
    }
    if let Some(limit) = s.regex_size_limit {
        b.size_limit(limit);
    }
    if let Some(limit) = s.dfa_size_limit {
        b.dfa_size_limit(limit);
    }
    if s.binary != Binary::AsText && !s.null_data {
        b.ban_byte(Some(b'\x00'));
    }
    b.build_many(patterns).map_err(|e| e.to_string())
}

fn matcher_pcre2(s: &Search, patterns: &[String]) -> Result<grep_pcre2::RegexMatcher, String> {
    let mut b = grep_pcre2::RegexMatcherBuilder::new();
    b.multi_line(true).fixed_strings(s.fixed_strings);
    match s.case {
        Case::Sensitive => b.caseless(false),
        Case::Insensitive => b.caseless(true),
        Case::Smart => b.case_smart(true),
    };
    match s.boundary {
        Some(Boundary::Line) => {
            b.whole_line(true);
        }
        Some(Boundary::Word) => {
            b.word(true);
        }
        None => {}
    }
    b.jit_if_available(true).max_jit_stack_size(Some(10 * (1 << 20)));
    if !s.no_unicode {
        b.utf(true).ucp(true);
    }
    if s.multiline {
        b.dotall(s.multiline_dotall);
    }
    if s.crlf {
        b.crlf(true);
    }
    b.build_many(patterns).map_err(|e| e.to_string())
}

fn matcher(s: &Search, patterns: &[String]) -> AnyMatcher {
    let fail = |e: String| -> ! { die(&format!("rg: {e}")) };
    match s.engine {
        Engine::Default => AnyMatcher::Rust(matcher_rust(s, patterns).unwrap_or_else(|e| fail(e))),
        Engine::Pcre2 => AnyMatcher::Pcre2(matcher_pcre2(s, patterns).unwrap_or_else(|e| fail(e))),
        Engine::Auto => match matcher_rust(s, patterns) {
            Ok(m) => AnyMatcher::Rust(m),
            Err(rust) => match matcher_pcre2(s, patterns) {
                Ok(m) => AnyMatcher::Pcre2(m),
                Err(pcre) => fail(format!(
                    "regex could not be compiled with either the default regex engine or with PCRE2.\n\n\
                     default regex engine error:\n{rust}\n\nPCRE2 regex engine error:\n{pcre}"
                )),
            },
        },
    }
}

fn searcher(s: &Search, mmap: bool) -> SearcherBuilder {
    let mut b = SearcherBuilder::new();
    let term = if s.crlf {
        grep_matcher::LineTerminator::crlf()
    } else if s.null_data {
        grep_matcher::LineTerminator::byte(b'\x00')
    } else {
        grep_matcher::LineTerminator::byte(b'\n')
    };
    b.stop_on_nonmatch(s.stop_on_nonmatch);
    b.max_matches(s.max_count)
        .line_terminator(term)
        .invert_match(s.invert)
        .line_number(true)
        .multi_line(s.multiline)
        // SAFETY: as in rg, a memory map is only read, and a file truncated while mapped can at worst end the process
        .memory_map(if mmap { unsafe { MmapChoice::auto() } } else { MmapChoice::never() });
    let (before, after) = s.context();
    b.before_context(before).after_context(after);
    match &s.encoding {
        Encoding::Auto => {}
        Encoding::Label(label) => {
            b.encoding(Some(grep_searcher::Encoding::new(label).expect("checked when the flag was read")));
        }
        Encoding::Disabled => {
            b.bom_sniffing(false);
        }
    }
    b
}

/// rg's binary detection for a file the user named and for one the walk found.
fn binary(s: &Search) -> (BinaryDetection, BinaryDetection) {
    if s.null_data {
        return (BinaryDetection::none(), BinaryDetection::none());
    }
    match s.binary {
        Binary::AsText => (BinaryDetection::none(), BinaryDetection::none()),
        Binary::SearchAndSuppress => (BinaryDetection::convert(0), BinaryDetection::convert(0)),
        Binary::Auto => (BinaryDetection::convert(0), BinaryDetection::quit(0)),
    }
}

/// The walk's own filters: -g and --iglob, then -t, -T and the other type flags.
struct Filters {
    globs: ignore::overrides::Override,
    types: ignore::types::Types,
}

impl Filters {
    fn new(s: &Search, cwd: &Path) -> Filters {
        let fail = |e: ignore::Error| -> ! { die(&format!("rg: {e}")) };
        let globs = if s.globs.is_empty() && s.iglobs.is_empty() {
            ignore::overrides::Override::empty()
        } else {
            let mut o = ignore::overrides::OverrideBuilder::new(cwd);
            if s.glob_case_insensitive {
                o.case_insensitive(true).unwrap_or_else(|e| fail(e));
            }
            for g in &s.globs {
                o.add(g).unwrap_or_else(|e| fail(e));
            }
            o.case_insensitive(true).unwrap_or_else(|e| fail(e));
            for g in &s.iglobs {
                o.add(g).unwrap_or_else(|e| fail(e));
            }
            o.build().unwrap_or_else(|e| fail(e))
        };
        let mut types = ignore::types::TypesBuilder::new();
        types.add_defaults();
        for change in &s.types {
            match change {
                TypeChange::Clear(name) => {
                    types.clear(name);
                }
                TypeChange::Add(def) => {
                    types.add_def(def).unwrap_or_else(|e| fail(e));
                }
                TypeChange::Select(name) => {
                    types.select(name);
                }
                TypeChange::Negate(name) => {
                    types.negate(name);
                }
            }
        }
        Filters { globs, types: types.build().unwrap_or_else(|e| fail(e)) }
    }
}

/// The walk of PATHS, and the messages about the --ignore-file files it could not read.
fn walker(
    s: &Search,
    filters: &Filters,
    paths: &[PathBuf],
    cwd: &Path,
    max_depth: Option<usize>,
) -> (WalkBuilder, Vec<String>) {
    let mut b = WalkBuilder::new(&paths[0]);
    for p in &paths[1..] {
        b.add(p);
    }
    let mut problems = Vec::new();
    if !s.no_ignore_files {
        problems.extend(s.ignore_files.iter().filter_map(|f| b.add_ignore(f)).map(|e| e.to_string()));
    }
    let threads = s.threads.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()).min(12));
    b.max_depth(max_depth)
        .follow_links(s.follow)
        .max_filesize(s.max_filesize)
        .threads(threads)
        .same_file_system(s.one_file_system)
        .overrides(filters.globs.clone())
        .types(filters.types.clone())
        .hidden(!s.hidden)
        .parents(!s.no_ignore_parent)
        .ignore(!s.no_ignore_dot)
        .git_global(!s.no_ignore_vcs && !s.no_ignore_global)
        .git_ignore(!s.no_ignore_vcs)
        .git_exclude(!s.no_ignore_vcs && !s.no_ignore_exclude)
        .require_git(!s.no_require_git)
        .ignore_case_insensitive(s.ignore_file_case_insensitive)
        .current_dir(cwd);
    if !s.no_ignore_dot {
        b.add_custom_ignore_filename(".rgignore");
    }
    (b, problems)
}

/// What one search pass gathers, shared by its walker's threads.
struct Pass<'a> {
    cap: u64,
    digest: Option<Digest>,
    cwd: &'a [u8],
    /// rg searched ./ because no PATH was given, and names what it finds there without the ./
    implicit: bool,
    /// only these files count, when given
    keep: Option<&'a HashSet<Vec<u8>>>,
    /// the deepest files searched, when the walk goes deeper than the search to see every file it would take
    max_depth: Option<usize>,
    /// whether to search the files at all, rather than only record them in VISITED
    search: bool,
    visited: Option<&'a Visited>,
    /// the files whose rows already count toward the cap, so a file searched twice counts once
    claimed: &'a Mutex<HashSet<Vec<u8>>>,
    collected: &'a AtomicU64,
    stop: &'a AtomicBool,
    searched: AtomicU64,
    /// whether the walk gave any file to search, which rg reports when it gave none
    any_file: AtomicBool,
    errored: AtomicBool,
    /// rg's --no-messages and --no-ignore-messages: an error still counts, but is not printed
    print_errors: bool,
    print_ignore_errors: bool,
    messages: Mutex<Vec<String>>,
    results: Mutex<Vec<(Vec<u8>, u64, Vec<Row>)>>,
}

impl Pass<'_> {
    fn message(&self, msg: String, error: bool) {
        if error {
            self.errored.store(true, Ordering::Relaxed);
        }
        if self.print_errors && (error || self.print_ignore_errors) {
            self.messages.lock().unwrap().push(format!("rg: {msg}"));
        }
    }
}

/// The files a walk took, by real path: the real path of the root it found each under, then the rest of its path.
/// Without -L the walk follows no symlink below a root, so this is the file's own real path.
struct Visited {
    roots: Vec<(PathBuf, Vec<u8>)>,
    files: Mutex<HashSet<Vec<u8>>>,
}

impl Visited {
    fn new(paths: &[PathBuf]) -> Visited {
        let roots = paths
            .iter()
            .map(|p| {
                let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                (p.clone(), real.into_os_string().into_encoded_bytes())
            })
            .collect();
        Visited { roots, files: Mutex::new(HashSet::new()) }
    }

    /// The real path of PATH, a file the walk found under one of the roots.
    fn real(&self, path: &Path) -> Option<Vec<u8>> {
        let found = self.roots.iter().filter_map(|(root, real)| Some((path.strip_prefix(root).ok()?, real)));
        let (rest, real) = found.max_by_key(|(rest, _)| std::cmp::Reverse(rest.as_os_str().len()))?;
        let rest = rest.as_os_str().as_bytes();
        Some(if rest.is_empty() { real.clone() } else { [real.as_slice(), b"/", rest].concat() })
    }
}

/// One walker thread's share of a [`Visited`], added to it when the thread's walk ends.
struct Recorder<'a> {
    into: Option<&'a Visited>,
    files: Vec<Vec<u8>>,
}

impl Recorder<'_> {
    fn record(&mut self, path: &Path) {
        if let Some(file) = self.into.and_then(|v| v.real(path)) {
            self.files.push(file);
        }
    }
}

impl Drop for Recorder<'_> {
    fn drop(&mut self) {
        if let Some(v) = self.into {
            v.files.lock().unwrap().extend(self.files.drain(..));
        }
    }
}

/// The part of rg's JSON printer explore reads: counts, and rows while they are wanted.
struct Collect<'a, M> {
    matcher: &'a M,
    want_rows: bool,
    count_matches: bool,
    cap: u64,
    matched_lines: u64,
    matches: u64,
    match_rows: u64,
    rows: Vec<Row>,
}

/// grep-printer's `find_iter_at_in_context` and `record_matches`: the matches of a reported line, as byte ranges
/// relative to the line. Only the first is looked for unless ALL.
fn submatches<M: Matcher>(searcher: &Searcher, m: &M, buf: &[u8], range: Range<usize>, all: bool) -> Vec<(usize, usize)> {
    let mut bytes = buf;
    if searcher.multi_line_with_matcher(m) {
        if bytes[range.end..].len() >= MAX_LOOK_AHEAD {
            bytes = &bytes[..range.end + MAX_LOOK_AHEAD];
        }
    } else {
        // the line terminator goes, so that look-around cannot see it
        let term = searcher.line_terminator();
        let mut end = range.end;
        if term.is_suffix(&buf[..end]) {
            end -= 1;
            if term.is_crlf() && end > 0 && buf.get(end - 1) == Some(&b'\r') {
                end -= 1;
            }
        }
        bytes = &buf[..end];
    }
    let mut found = Vec::new();
    let _ = m.find_iter_at(bytes, range.start, |mat| {
        if mat.start() >= range.end {
            return false;
        }
        found.push((mat.start() - range.start, mat.end() - range.start));
        all
    });
    // an empty match at the end of the buffer is not reported
    if found.last().is_some_and(|&(start, end)| start == end && start >= buf.len()) {
        found.pop();
    }
    found
}

/// Python's `text_of` and `submatches`: the text of rg's `lines`, as UTF-8 or else Latin-1, its first line, and
/// SUBS, byte ranges in it, as characters. A byte offset inside a character counts the characters before it.
fn row(bytes: &[u8], subs: &[(usize, usize)], line: u64, is_match: bool) -> Row {
    let (text, chars): (String, Vec<usize>) = match std::str::from_utf8(bytes) {
        Ok(t) => {
            let mut counted = (0, 0);
            let chars = subs
                .iter()
                .flat_map(|&(s, e)| [s, e])
                .map(|offset| {
                    let mut at = offset.min(t.len());
                    while !t.is_char_boundary(at) {
                        at -= 1;
                    }
                    // offsets ascend, so the count carries on from the last one
                    counted.1 += t[counted.0.min(at)..at].chars().count();
                    counted.0 = counted.0.max(at);
                    counted.1
                })
                .collect();
            (t.to_string(), chars)
        }
        Err(_) => (encoding_rs::mem::decode_latin1(bytes).into_owned(), subs.iter().flat_map(|&(s, e)| [s, e]).collect()),
    };
    let subs: Vec<Sub> = chars
        .chunks(2)
        .map(|c| {
            let (col, end) = (c[0], c[1]);
            let before: Vec<char> = text.chars().take(col).collect();
            let below = before.iter().filter(|&&ch| ch == '\n').count();
            let at = if below > 0 { col - before.iter().rposition(|&ch| ch == '\n').expect("a newline") - 1 } else { col };
            Sub { col, end, below, at }
        })
        .collect();
    let column = subs.first().map_or(0, |s| s.col);
    let first = text.trim_end_matches('\n').split('\n').next().unwrap_or("").to_string();
    Row { line, text: first, is_match, column, subs }
}

impl<M: Matcher> Sink for Collect<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.matched_lines += mat.lines().count() as u64;
        if self.want_rows || self.count_matches {
            let all = self.count_matches || self.want_rows;
            let subs = submatches(searcher, self.matcher, mat.buffer(), mat.bytes_range_in_buffer(), all);
            self.matches += subs.len() as u64;
            if self.want_rows {
                let line = mat.line_number().expect("line numbers are on");
                self.rows.push(row(mat.bytes(), &subs, line, true));
                self.match_rows += 1;
                // past the cap no row is printed, only the counts
                self.want_rows = self.match_rows <= self.cap;
            }
        }
        Ok(true)
    }

    fn context(&mut self, _: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, Self::Error> {
        if self.want_rows {
            self.rows.push(row(ctx.bytes(), &[], ctx.line_number().expect("line numbers are on"), false));
        }
        Ok(true)
    }

    fn finish(&mut self, _: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn is_dir(dent: &DirEntry) -> bool {
    match dent.file_type() {
        None => false,
        Some(ft) => ft.is_dir() || (dent.path_is_symlink() && dent.path().is_dir()),
    }
}

/// How a file's contents reach the searcher, as in rg: through the --pre command for the files --pre-glob takes,
/// through a decompression command with -z for a compressed file, and otherwise read from the file.
struct Source {
    pre: Option<(PathBuf, ignore::overrides::Override)>,
    zip: Option<grep_cli::DecompressionReaderBuilder>,
    command: grep_cli::CommandReaderBuilder,
}

impl Source {
    fn new(s: &Search, cwd: &Path) -> Source {
        let pre = s.pre.clone().map(|bin| {
            let mut globs = ignore::overrides::OverrideBuilder::new(cwd);
            for g in &s.pre_globs {
                globs.add(g).unwrap_or_else(|e| die(&format!("rg: {e}")));
            }
            (bin, globs.build().unwrap_or_else(|e| die(&format!("rg: {e}"))))
        });
        let zip = s.search_zip.then(|| {
            let mut b = grep_cli::DecompressionReaderBuilder::new();
            b.async_stderr(true);
            b
        });
        let mut command = grep_cli::CommandReaderBuilder::new();
        command.async_stderr(true);
        Source { pre, zip, command }
    }

    fn search<M: Matcher, S: Sink<Error = std::io::Error>>(
        &self,
        searcher: &mut Searcher,
        m: &M,
        path: &Path,
        sink: S,
    ) -> std::io::Result<()> {
        if let Some((bin, globs)) = &self.pre
            && (globs.is_empty() || !globs.matched(path, false).is_ignore())
        {
            let mut cmd = Command::new(bin);
            cmd.arg(path).stdin(Stdio::from(std::fs::File::open(path)?));
            let mut rdr = self.command.build(&mut cmd).map_err(|e| {
                std::io::Error::other(format!("preprocessor command could not start: '{cmd:?}': {e}"))
            })?;
            let result = searcher
                .search_reader(m, &mut rdr, sink)
                .map_err(|e| std::io::Error::other(format!("preprocessor command failed: '{cmd:?}': {e}")));
            let closed = rdr.close();
            result?;
            return closed;
        }
        if let Some(zip) = &self.zip
            && zip.get_matcher().has_command(path)
        {
            let mut rdr = zip.build(path)?;
            let result = searcher.search_reader(m, &mut rdr, sink);
            let closed = rdr.close();
            result?;
            return closed;
        }
        searcher.search_path(m, path, sink)
    }
}

fn search_entry<M: Matcher>(
    pass: &Pass,
    m: &M,
    source: &Source,
    recorder: &mut Recorder,
    searcher: &mut Searcher,
    (explicit_bin, implicit_bin): &(BinaryDetection, BinaryDetection),
    result: Result<DirEntry, ignore::Error>,
) {
    let dent = match result {
        Ok(d) => d,
        Err(e) => return pass.message(e.to_string(), true),
    };
    if let Some(e) = dent.error() {
        pass.message(e.to_string(), false);
    }
    let explicit = dent.depth() == 0 && !is_dir(&dent);
    if !explicit && !dent.file_type().is_some_and(|ft| ft.is_file()) {
        return;
    }
    recorder.record(dent.path());
    if !pass.search || pass.max_depth.is_some_and(|d| dent.depth() > d) {
        return;
    }
    pass.any_file.store(true, Ordering::Relaxed);
    let path = display_path(dent.path(), pass.cwd);
    let kept = pass.keep.is_none_or(|k| k.contains(&path));
    searcher.set_binary_detection(if explicit { explicit_bin.clone() } else { implicit_bin.clone() });
    let mut sink = Collect {
        matcher: m,
        want_rows: kept && pass.digest.is_none() && !pass.stop.load(Ordering::Relaxed),
        count_matches: pass.digest == Some(Digest::Matches),
        cap: pass.cap,
        matched_lines: 0,
        matches: 0,
        match_rows: 0,
        rows: Vec::new(),
    };
    let shown = if pass.implicit { dent.path().strip_prefix("./").unwrap_or(dent.path()) } else { dent.path() };
    if let Err(e) = source.search(searcher, m, dent.path(), &mut sink) {
        return pass.message(format!("{}: {e}", shown.display()), true);
    }
    if pass.keep.is_none() {
        pass.searched.fetch_add(1, Ordering::Relaxed);
    }
    if !kept {
        return;
    }
    if sink.match_rows > 0 && pass.claimed.lock().unwrap().insert(path.clone()) {
        let total = pass.collected.fetch_add(sink.match_rows, Ordering::Relaxed) + sink.match_rows;
        if total > pass.cap {
            pass.stop.store(true, Ordering::Relaxed);
        }
    }
    let stat = if pass.digest == Some(Digest::Matches) { sink.matches } else { sink.matched_lines };
    // what a pass holds grows with the files that match and the rows kept, never with the search's output
    if stat != 0 || !sink.rows.is_empty() {
        pass.results.lock().unwrap().push((path, stat, sink.rows));
    }
}

fn run_pass<M: Matcher + Sync>(pass: &Pass, m: &M, s: &Search, (walk, problems): (WalkBuilder, Vec<String>), mmap: bool) {
    for p in problems {
        pass.message(p, false);
    }
    let builder = searcher(s, mmap);
    let bins = &binary(s);
    let source = &Source::new(s, Path::new(os(pass.cwd)));
    walk.build_parallel().run(|| {
        let mut searcher = builder.build();
        let mut recorder = Recorder { into: pass.visited, files: Vec::new() };
        Box::new(move |result| {
            search_entry(pass, m, source, &mut recorder, &mut searcher, bins, result);
            WalkState::Continue
        })
    });
}

fn git(args: &[&[u8]]) -> std::process::Output {
    let mut cmd = Command::new("git");
    for a in args {
        cmd.arg(os(a));
    }
    cmd.stdin(Stdio::null()).output().unwrap_or_else(|e| die(&format!("git did not run: {e}")))
}

fn git_top() -> Option<Vec<u8>> {
    let out = git(&[b"rev-parse", b"--show-toplevel"]);
    if !out.status.success() {
        return None;
    }
    let top = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(std::fs::canonicalize(&top).map_or(top.into_bytes(), |p| p.into_os_string().into_encoded_bytes()))
}

/// The real paths of the PATHS inside the repository at TOP, or of the working directory CWD without PATHS: where
/// the tracked files .gitignore hides are looked for.
fn scope(paths: &[Vec<u8>], top: &[u8], cwd: &[u8]) -> Vec<Vec<u8>> {
    if paths.is_empty() {
        return vec![cwd.to_vec()];
    }
    let under = |r: &Vec<u8>| [r.as_slice(), b"/"].concat().starts_with(&[top, b"/"].concat());
    paths
        .iter()
        .map(|p| std::fs::canonicalize(os(p)).map_or(p.clone(), |r| r.into_os_string().into_encoded_bytes()))
        .filter(under)
        .collect()
}

/// The files git tracks under INSIDE, the scope in the repository at TOP, as paths from the working directory.
fn tracked(inside: &[Vec<u8>], top: &[u8]) -> Vec<Vec<u8>> {
    if inside.is_empty() {
        return Vec::new();
    }
    // each pathspec is relative to TOP, through any symlink, and literal, so a name holding * or [ matches only
    // itself; TOP itself is the empty path, since a literal . matches no file
    let specs: Vec<Vec<u8>> = inside
        .iter()
        .map(|r| [&b":(top,literal)"[..], if r.as_slice() == top { b"" } else { &r[top.len() + 1..] }].concat())
        .collect();
    let mut args: Vec<&[u8]> = vec![b"ls-files", b"-z", b"--"];
    args.extend(specs.iter().map(Vec::as_slice));
    let out = git(&args);
    if !out.status.success() {
        // without this list the tracked files .gitignore hides would drop out of the result unannounced
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.trim().lines().next().unwrap_or("no message").to_string();
        let code = out.status.code().map_or("a signal".to_string(), |c| c.to_string());
        die(&format!("git ls-files exited with {code}: {first}"));
    }
    out.stdout.split(|&b| b == 0).filter(|n| !n.is_empty()).map(py::normpath).collect()
}

/// The tracked files .gitignore hides: those of TRACKED the walk did not take. A submodule is a directory to git
/// ls-files and a symlink is not walked, so neither is a file the walk skipped.
fn hidden_tracked(tracked: Vec<Vec<u8>>, visited: &Visited, cwd: &[u8]) -> HashSet<Vec<u8>> {
    let visited = visited.files.lock().unwrap();
    tracked
        .into_iter()
        .filter(|f| !visited.contains(&py::abspath(f, cwd)))
        .filter(|f| std::fs::symlink_metadata(os(f)).is_ok_and(|m| m.is_file()))
        .collect()
}

/// Whether a walk with the settings S takes the same files as one with BASE's: what the user's flags change about
/// it besides following symlinks, which reach no tracked path, and its depth, which the walk ignores when it records.
fn same_files(s: &Search, base: &Search) -> bool {
    s.globs == base.globs
        && s.iglobs.is_empty()
        && s.types.is_empty()
        && s.max_filesize.is_none()
        && s.hidden == base.hidden
        && s.one_file_system == base.one_file_system
        && (s.no_ignore_dot, s.no_ignore_exclude, s.no_ignore_global, s.no_ignore_parent, s.no_ignore_vcs)
            == (base.no_ignore_dot, base.no_ignore_exclude, base.no_ignore_global, base.no_ignore_parent, base.no_ignore_vcs)
        && s.no_require_git == base.no_require_git
        && (s.ignore_files.is_empty() || s.no_ignore_files)
        && !s.ignore_file_case_insensitive
}

/// Both searches, with Python's explore's flags for each.
pub fn search(hits: &Hits) -> Found {
    let cwd_path = std::env::current_dir().unwrap_or_else(|e| die(&format!("no working directory: {e}")));
    let cwd = cwd_path.as_os_str().as_bytes().to_vec();
    let rg = |prefix: &[&[u8]], suffix: &[&[u8]]| {
        let mut flags: Vec<Vec<u8>> = prefix.iter().map(|f| f.to_vec()).collect();
        flags.extend(hits.flags.iter().cloned());
        flags.extend(suffix.iter().map(|f| f.to_vec()));
        rg_flags(&flags)
    };
    let walk = rg(&[b"--hidden", b"--no-ignore-dot", b"-g", b"!.git"], &[]);
    // as in rg: -f patterns in order, then -e patterns, without repeats; each must be UTF-8
    let mut patterns: Vec<String> = Vec::new();
    for f in &walk.pattern_files {
        let read = if f.as_os_str() == "-" { Ok(Vec::new()) } else { grep_cli::patterns_from_path(f) };
        patterns.extend(read.unwrap_or_else(|e| die(&format!("rg: {e}"))));
    }
    for p in &hits.patterns {
        patterns.push(String::from_utf8(p.clone()).unwrap_or_else(|_| die("rg: pattern given is not valid UTF-8")));
    }
    let mut seen = HashSet::new();
    patterns.retain(|p| seen.insert(p.clone()));

    let top = git_top();
    let inside = top.as_deref().map_or_else(Vec::new, |t| scope(&hits.paths, t, &cwd));
    // git lists the index while the walk runs
    let listing = {
        let (inside, top) = (inside.clone(), top.clone());
        std::thread::spawn(move || top.as_deref().map_or_else(Vec::new, |t| tracked(&inside, t)))
    };
    let implicit = hits.paths.is_empty();
    let paths: Vec<PathBuf> =
        if implicit { vec![PathBuf::from("./")] } else { hits.paths.iter().map(|p| PathBuf::from(os(p))).collect() };
    let visited = Visited::new(&paths);
    let roots: Vec<PathBuf> = inside.iter().map(|r| PathBuf::from(os(r))).collect();
    let own = Visited::new(&roots);
    // the hidden files are the tracked ones a walk with only explore's own flags skips; the search's walk records
    // what it takes when the user's flags leave that the same, and a walk of its own records it otherwise
    let base = rg_flags(&[b"--hidden".to_vec(), b"--no-ignore-dot".to_vec(), b"-g".to_vec(), b"!.git".to_vec()]);
    let record = top.is_some() && same_files(&walk, &base);
    // rg searches nothing when no match is possible
    let possible = |s: &Search| (!patterns.is_empty() || s.invert) && s.max_count != Some(0);
    let (claimed, collected, stop) = (Mutex::new(HashSet::new()), AtomicU64::new(0), AtomicBool::new(false));
    // --json keeps the rows past the cap, up to SHAPE_CAP, to shape them
    let cap = if hits.json { hits.cap.max(crate::hits::SHAPE_CAP) } else { hits.cap };
    let pass = |keep, implicit, max_depth, search, visited| Pass {
        cap,
        digest: hits.digest,
        cwd: &cwd,
        implicit,
        keep,
        max_depth,
        search,
        visited,
        claimed: &claimed,
        collected: &collected,
        stop: &stop,
        searched: AtomicU64::new(0),
        any_file: AtomicBool::new(false),
        errored: AtomicBool::new(false),
        print_errors: true,
        print_ignore_errors: true,
        messages: Mutex::new(Vec::new()),
        results: Mutex::new(Vec::new()),
    };
    // to tell which tracked files the ignore files hide, the walk goes below --max-depth, searching nothing there,
    // and runs even when no match is possible
    let walk_filters = Filters::new(&walk, &cwd_path);
    let walk_depth = if record { None } else { walk.max_depth };
    let mut first = pass(None, implicit, walk.max_depth, possible(&walk), record.then_some(&visited));
    (first.print_errors, first.print_ignore_errors) = (walk.messages, walk.ignore_messages);
    if possible(&walk) || record {
        let mmap = match walk.mmap {
            Mmap::Auto => paths.len() <= 10 && paths.iter().all(|p| p.is_file()),
            Mmap::Always => true,
            Mmap::Never => false,
        };
        let w = walker(&walk, &walk_filters, &paths, &cwd_path, walk_depth);
        if possible(&walk) {
            match matcher(&walk, &patterns) {
                AnyMatcher::Rust(m) => run_pass(&first, &m, &walk, w, mmap),
                AnyMatcher::Pcre2(m) => run_pass(&first, &m, &walk, w, mmap),
            }
        } else {
            run_pass(&first, &grep_regex::RegexMatcher::new("").expect("an empty regex builds"), &walk, w, mmap);
        }
        if possible(&walk) && implicit && !first.any_file.load(Ordering::Relaxed) {
            first.message(
                "No files were searched, which means ripgrep probably applied a filter you didn't expect.\n\
                 Running with --debug will show why files are being skipped."
                    .into(),
                true,
            );
        }
    }
    let tracked_files = listing.join().expect("git ls-files does not panic");
    let wanted = !tracked_files.is_empty();
    let record = record && wanted;
    let tracked_search = rg(&[b"--no-ignore", b"--hidden"], &[b"--max-depth", b"1"]);
    let hidden = if record {
        hidden_tracked(tracked_files, &visited, &cwd)
    } else if wanted {
        let recorder = pass(None, false, None, false, Some(&own));
        let w = walker(&base, &Filters::new(&base, &cwd_path), &roots, &cwd_path, None);
        run_pass(&recorder, &grep_regex::RegexMatcher::new("").expect("an empty regex builds"), &base, w, false);
        hidden_tracked(tracked_files, &own, &cwd)
    } else {
        HashSet::new()
    };
    let mut second = pass(Some(&hidden), false, None, true, None);
    (second.print_errors, second.print_ignore_errors) = (tracked_search.messages, tracked_search.ignore_messages);
    if !hidden.is_empty() && possible(&tracked_search) {
        let mut roots: Vec<Vec<u8>> =
            hidden.iter().map(|f| py::dirname(f)).map(|d| if d.is_empty() { b".".to_vec() } else { d.to_vec() }).collect();
        roots.sort_by_cached_key(|r| py::sort_key(r));
        roots.dedup();
        let roots: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(os(r))).collect();
        let filters = Filters::new(&tracked_search, &cwd_path);
        let w = walker(&tracked_search, &filters, &roots, &cwd_path, tracked_search.max_depth);
        match matcher(&tracked_search, &patterns) {
            AnyMatcher::Rust(m) => run_pass(&second, &m, &tracked_search, w, tracked_search.mmap == Mmap::Always),
            AnyMatcher::Pcre2(m) => run_pass(&second, &m, &tracked_search, w, tracked_search.mmap == Mmap::Always),
        }
    }

    let mut found = Found {
        per_file: HashMap::new(),
        rows: HashMap::new(),
        searched: first.searched.load(Ordering::Relaxed),
        errors: Vec::new(),
        hidden: HashSet::new(),
    };
    for p in [first, second] {
        if p.errored.load(Ordering::Relaxed) {
            for msg in p.messages.into_inner().unwrap() {
                found.errors.extend(msg.lines().filter(|l| !l.trim().is_empty()).map(str::to_string));
            }
        }
        for (path, stat, rows) in p.results.into_inner().unwrap() {
            if stat != 0 {
                found.per_file.insert(path.clone(), stat);
            }
            if !rows.is_empty() {
                found.rows.entry(path).or_default().extend(rows);
            }
        }
    }
    found.hidden = hidden;
    found
}
