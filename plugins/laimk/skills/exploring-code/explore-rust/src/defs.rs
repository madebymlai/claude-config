//! Definitions, from a tree-sitter parse of each file.
//!
//! Python's explore asks ast-grep 0.51.3 for every node of a definition kind, with the line of the node's name.
//! This module walks the same grammars' trees for the same kinds and reproduces ast-grep's reasons for not parsing
//! a file, so each file gets the same definitions or the same label.

use std::collections::HashMap;
use std::path::Path;

use tree_sitter::{Language, Node, Parser};

use crate::py;

/// One definition: 1-based inclusive lines, from the first of its leads, and the line holding its name, stripped.
pub struct Def {
    /// the node kind, such as function_item
    pub kind: String,
    /// what names it: the text of its name child, or for a Rust impl its type; a C or C++ function's declarator,
    /// such as `*Foo::bar(int x)`, names it by what precedes its parameters; none for a definition with neither
    pub name: Option<String>,
    pub start: usize,
    pub end: usize,
    pub sig: String,
    /// the line holding its name, or its node's first line when it has none
    pub line: usize,
}

/// A comment or a string, or code put back inside a string, with 0-based lines and columns in characters, as
/// ast-grep reports a node's range.
#[derive(Clone, Copy)]
pub struct Place {
    pub start: (usize, usize),
    pub end: (usize, usize),
    /// "comment", "string" or "code"
    pub what: &'static str,
}

/// The Python script's PLACES: per language, the node kinds a match sits in when it is in a comment or a string,
/// and those that put code back inside a string, such as an interpolation.
const PLACES: &[(&str, &str, &str, &str)] = &[
    ("rust", "line_comment block_comment", "string_literal raw_string_literal char_literal", ""),
    ("java", "line_comment block_comment", "string_literal character_literal", "string_interpolation"),
    ("kotlin", "line_comment multiline_comment", "string_literal character_literal", "interpolated_expression interpolated_identifier"),
    ("python", "comment", "string", "interpolation"),
    ("javascript", "comment html_comment", "string template_string regex jsx_text", "template_substitution"),
    ("typescript", "comment", "string template_string regex", "template_substitution"),
    ("tsx", "comment", "string template_string regex jsx_text", "template_substitution"),
    ("go", "comment", "interpreted_string_literal raw_string_literal rune_literal", ""),
    (
        "csharp",
        "comment",
        "string_literal verbatim_string_literal raw_string_literal interpolated_string_expression character_literal",
        "interpolation",
    ),
    ("c", "comment", "string_literal char_literal system_lib_string", ""),
    ("cpp", "comment", "string_literal char_literal raw_string_literal system_lib_string", ""),
    ("ruby", "comment", "string heredoc_body regex character subshell delimited_symbol string_array", "interpolation"),
    ("php", "comment", "string encapsed_string heredoc nowdoc", ""),
    ("swift", "comment multiline_comment", "line_string_literal multi_line_string_literal raw_string_literal", "interpolated_expression"),
    ("scala", "comment block_comment", "string interpolated_string interpolated_string_expression character_literal", "interpolation"),
    ("lua", "comment", "string", ""),
    ("bash", "comment", "string raw_string ansi_c_string heredoc_body", "command_substitution expansion simple_expansion"),
];

/// What a file's parse gave: its definitions sorted by start and then by end, outermost first, or why it was not
/// parsed, worded as ast-grep reports it.
pub enum Parsed {
    Defs(Vec<Def>),
    Skipped(String),
}

pub struct Lang {
    name: &'static str,
    /// the extensions Python's explore maps to the language, matched case-insensitively
    extensions: &'static [&'static str],
    /// the extensions ast-grep maps to it, matched exactly; a file Python maps here whose extension is not one of
    /// these is one ast-grep finds no language for
    ast_grep_extensions: &'static [&'static str],
    kinds: &'static [&'static str],
    /// a variable_declarator holding an arrow_function is a definition too
    arrow: bool,
    grammar: fn() -> Language,
}

#[derive(Clone, Copy)]
enum NameChild {
    Field(&'static str),
    /// Kotlin's grammar names no fields: the first child of one of these kinds
    Kinds(&'static [&'static str]),
}

const NAME_FIELD: &[NameChild] = &[NameChild::Field("name")];
const C_NAME: &[NameChild] = &[NameChild::Field("name"), NameChild::Field("declarator")];
const KOTLIN_NAME: &[NameChild] = &[NameChild::Kinds(&["simple_identifier", "type_identifier"])];

/// Python's LANGUAGES table, in its order, with ast-grep's extensions from its language registry.
pub static LANGUAGES: &[Lang] = &[
    Lang {
        name: "rust",
        extensions: &["rs"],
        ast_grep_extensions: &["rs"],
        kinds: &["function_item", "impl_item", "trait_item", "struct_item", "enum_item", "mod_item", "macro_definition", "union_item"],
        arrow: false,
        grammar: || tree_sitter_rust::LANGUAGE.into(),
    },
    Lang {
        name: "java",
        extensions: &["java"],
        ast_grep_extensions: &["java"],
        kinds: &["class_declaration", "interface_declaration", "enum_declaration", "record_declaration", "annotation_type_declaration", "method_declaration", "constructor_declaration"],
        arrow: false,
        grammar: || tree_sitter_java::LANGUAGE.into(),
    },
    Lang {
        name: "kotlin",
        extensions: &["kt", "kts"],
        ast_grep_extensions: &["kt", "ktm", "kts"],
        kinds: &["class_declaration", "object_declaration", "function_declaration"],
        arrow: false,
        grammar: || tree_sitter_kotlin_sg::LANGUAGE.into(),
    },
    Lang {
        name: "python",
        extensions: &["py", "pyi"],
        ast_grep_extensions: &["py", "py3", "pyi", "bzl", "bazel"],
        kinds: &["function_definition", "class_definition"],
        arrow: false,
        grammar: || tree_sitter_python::LANGUAGE.into(),
    },
    Lang {
        name: "javascript",
        extensions: &["js", "mjs", "cjs", "jsx"],
        ast_grep_extensions: &["cjs", "js", "mjs", "jsx"],
        kinds: &["function_declaration", "generator_function_declaration", "class_declaration", "method_definition"],
        arrow: true,
        grammar: || tree_sitter_javascript::LANGUAGE.into(),
    },
    Lang {
        name: "typescript",
        extensions: &["ts", "mts", "cts"],
        ast_grep_extensions: &["ts", "cts", "mts"],
        kinds: &["function_declaration", "generator_function_declaration", "class_declaration", "abstract_class_declaration", "method_definition", "interface_declaration", "enum_declaration", "module", "internal_module"],
        arrow: true,
        grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    },
    Lang {
        name: "tsx",
        extensions: &["tsx"],
        ast_grep_extensions: &["tsx"],
        kinds: &["function_declaration", "class_declaration", "abstract_class_declaration", "method_definition", "interface_declaration", "enum_declaration", "internal_module"],
        arrow: true,
        grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
    },
    Lang {
        name: "go",
        extensions: &["go"],
        ast_grep_extensions: &["go"],
        kinds: &["function_declaration", "method_declaration", "type_declaration"],
        arrow: false,
        grammar: || tree_sitter_go::LANGUAGE.into(),
    },
    Lang {
        name: "csharp",
        extensions: &["cs"],
        ast_grep_extensions: &["cs"],
        kinds: &["namespace_declaration", "class_declaration", "struct_declaration", "interface_declaration", "enum_declaration", "record_declaration", "method_declaration", "constructor_declaration"],
        arrow: false,
        grammar: || tree_sitter_c_sharp::LANGUAGE.into(),
    },
    Lang {
        name: "c",
        extensions: &["c", "h"],
        ast_grep_extensions: &["c", "h"],
        kinds: &["function_definition", "struct_specifier"],
        arrow: false,
        grammar: || tree_sitter_c::LANGUAGE.into(),
    },
    Lang {
        name: "cpp",
        extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        ast_grep_extensions: &["cc", "hpp", "cpp", "c++", "hh", "cxx", "cu", "ino"],
        kinds: &["function_definition", "class_specifier", "struct_specifier", "namespace_definition"],
        arrow: false,
        grammar: || tree_sitter_cpp::LANGUAGE.into(),
    },
    Lang {
        name: "ruby",
        extensions: &["rb"],
        ast_grep_extensions: &["rb", "rbw", "gemspec"],
        kinds: &["class", "module", "method", "singleton_method"],
        arrow: false,
        grammar: || tree_sitter_ruby::LANGUAGE.into(),
    },
    Lang {
        name: "php",
        extensions: &["php"],
        ast_grep_extensions: &["php"],
        kinds: &["class_declaration", "interface_declaration", "trait_declaration", "function_definition", "method_declaration"],
        arrow: false,
        grammar: || tree_sitter_php::LANGUAGE_PHP.into(),
    },
    Lang {
        name: "swift",
        extensions: &["swift"],
        ast_grep_extensions: &["swift"],
        kinds: &["class_declaration", "protocol_declaration", "function_declaration"],
        arrow: false,
        grammar: || tree_sitter_swift::LANGUAGE.into(),
    },
    Lang {
        name: "scala",
        extensions: &["scala", "sc"],
        ast_grep_extensions: &["scala", "sc", "sbt"],
        kinds: &["class_definition", "object_definition", "trait_definition", "function_definition"],
        arrow: false,
        grammar: || tree_sitter_scala::LANGUAGE.into(),
    },
    Lang {
        name: "lua",
        extensions: &["lua"],
        ast_grep_extensions: &["lua"],
        kinds: &["function_declaration"],
        arrow: false,
        grammar: || tree_sitter_lua::LANGUAGE.into(),
    },
    Lang {
        name: "bash",
        extensions: &["sh", "bash"],
        ast_grep_extensions: &["bash", "bats", "cgi", "command", "env", "fcgi", "ksh", "sh", "tmux", "tool", "zsh"],
        kinds: &["function_definition"],
        arrow: false,
        grammar: || tree_sitter_bash::LANGUAGE.into(),
    },
];

impl Lang {
    fn name_child(&self) -> &'static [NameChild] {
        match self.name {
            "c" | "cpp" => C_NAME,
            "kotlin" => KOTLIN_NAME,
            _ => NAME_FIELD,
        }
    }
}

/// The language Python's explore gives PATH: by its extension, lowercased.
pub fn language(path: &[u8]) -> Option<&'static Lang> {
    let ext = std::str::from_utf8(py::extension(path)).ok()?.to_lowercase();
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext.as_str()))
}

/// ast-grep's limits: it skips a file over 3,000,000 bytes that also has over 200,000 lines
const MAX_FILE_SIZE: usize = 3_000_000;
const MAX_LINE_COUNT: usize = 200_000;

/// The kind ids a language's definitions have, resolved once per run.
struct Compiled {
    language: Language,
    kinds: Vec<u16>,
    arrow_parent: u16,
    arrow: u16,
    name_fields: Vec<Option<u16>>,
    name_kinds: Vec<Vec<u16>>,
    type_field: Option<u16>,
    /// the place of each comment, string and interpolation kind
    places: HashMap<u16, &'static str>,
    /// what leads a definition from outside its node (Python's LEADS): Rust's attribute_item, with the comment kinds
    /// between it and the definition, or Python's decorated_definition
    lead: Option<Lead>,
}

enum Lead {
    /// an attribute_item, then any attribute_item or comment siblings, then the definition
    Rust { attribute: u16, skipped: [u16; 3] },
    /// a decorated_definition, whose definition field is the definition
    Python { decorated: u16, definition: u16 },
}

fn compile(lang: &Lang) -> Compiled {
    let language = (lang.grammar)();
    let kind = |k: &str| {
        let id = language.id_for_node_kind(k, true);
        assert!(id != 0, "{} has no node kind {k}", lang.name);
        id
    };
    let kinds = lang.kinds.iter().map(|k| kind(k)).collect();
    let (arrow_parent, arrow) = if lang.arrow { (kind("variable_declarator"), kind("arrow_function")) } else { (0, 0) };
    let mut name_fields = Vec::new();
    let mut name_kinds = Vec::new();
    for child in lang.name_child() {
        match child {
            NameChild::Field(f) => {
                // ast-grep refuses a rule naming a field its grammar lacks
                let id = language.field_id_for_name(f).unwrap_or_else(|| panic!("{} has no field {f}", lang.name));
                name_fields.push(Some(id.get()));
                name_kinds.push(Vec::new());
            }
            NameChild::Kinds(ks) => {
                name_fields.push(None);
                name_kinds.push(ks.iter().map(|k| kind(k)).collect());
            }
        }
    }
    let type_field = (lang.name == "rust").then(|| language.field_id_for_name("type").expect("rust has a type field").get());
    let (_, comment, string, code) = PLACES.iter().find(|p| p.0 == lang.name).expect("every language has its places");
    let mut places = HashMap::new();
    for (what, list) in [("comment", comment), ("string", string), ("code", code)] {
        for k in list.split_whitespace() {
            places.insert(kind(k), what);
        }
    }
    let lead = match lang.name {
        "rust" => Some(Lead::Rust {
            attribute: kind("attribute_item"),
            skipped: [kind("attribute_item"), kind("line_comment"), kind("block_comment")],
        }),
        "python" => Some(Lead::Python {
            decorated: kind("decorated_definition"),
            definition: language.field_id_for_name("definition").expect("python has a definition field").get(),
        }),
        _ => None,
    };
    Compiled { language, kinds, arrow_parent, arrow, name_fields, name_kinds, type_field, places, lead }
}

impl Compiled {
    fn is_definition(&self, node: Node) -> bool {
        let id = node.kind_id();
        self.kinds.contains(&id)
            || (self.arrow_parent != 0
                && id == self.arrow_parent
                && (0..node.child_count() as u32).filter_map(|i| node.child(i)).any(|c| c.kind_id() == self.arrow))
    }

    /// The line of the definition's name child, the first rule that finds one as in ast-grep's `any`; the name is
    /// a named node, as ast-grep's `$NAME` requires.
    /// A Rust impl's type, which names a definition that has no name child.
    fn type_node<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        node.child_by_field_id(self.type_field?).filter(Node::is_named)
    }

    /// The definition NODE leads, if it is a lead: for an attribute, the first sibling after it that is neither an
    /// attribute nor a comment, when that is a definition, as ast-grep's precedes with its stopBy finds it.
    fn led<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        match self.lead.as_ref()? {
            Lead::Rust { attribute, skipped } if node.kind_id() == *attribute => {
                let mut next = node.next_sibling();
                while let Some(n) = next.filter(|n| skipped.contains(&n.kind_id())) {
                    next = n.next_sibling();
                }
                next.filter(|n| self.is_definition(*n))
            }
            Lead::Python { decorated, definition } if node.kind_id() == *decorated => {
                node.child_by_field_id(*definition).filter(|n| self.is_definition(*n))
            }
            _ => None,
        }
    }

    fn name_node<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        for (field, kinds) in self.name_fields.iter().zip(&self.name_kinds) {
            let child = match field {
                Some(f) => node.child_by_field_id(*f),
                None if !kinds.is_empty() => {
                    (0..node.child_count() as u32).filter_map(|i| node.child(i)).find(|c| kinds.contains(&c.kind_id()))
                }
                None => None,
            };
            if let Some(c) = child.filter(Node::is_named) {
                return Some(c);
            }
        }
        None
    }
}

/// Byte columns as character columns, one line's table at a time; a line in ASCII needs none.
#[derive(Default)]
struct CharColumns {
    row: Option<usize>,
    /// the character column of each byte column of ROW, with one more for its end
    table: Vec<usize>,
}

impl CharColumns {
    fn of(&mut self, lines: &[String], row: usize, byte: usize) -> usize {
        let Some(line) = lines.get(row) else { return byte };
        if line.is_ascii() {
            return byte;
        }
        if self.row != Some(row) {
            self.table.clear();
            for (n, c) in line.chars().enumerate() {
                self.table.extend(std::iter::repeat_n(n, c.len_utf8()));
            }
            self.table.push(line.chars().count());
            self.row = Some(row);
        }
        self.table.get(byte).copied().unwrap_or(byte)
    }
}

/// The name TEXT gives: for C and C++ the declarator's words before its parameters, without pointer or reference
/// marks.
fn name_of(lang: &str, text: &str) -> String {
    if lang == "c" || lang == "cpp" {
        let head = text.split('(').next().unwrap_or("");
        head.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(['*', '&', ' ']).to_string()
    } else {
        text.to_string()
    }
}

/// Parses files, caching each language's compiled kinds and each file's result.
pub struct Definitions {
    compiled: HashMap<&'static str, Compiled>,
    pub files: HashMap<Vec<u8>, Option<Parsed>>,
    /// whether to find each parsed file's comments and strings too
    with_places: bool,
    /// the comments and strings of each file parsed with them
    pub places: HashMap<Vec<u8>, Vec<Place>>,
}

impl Definitions {
    pub fn new() -> Self {
        Definitions { compiled: HashMap::new(), files: HashMap::new(), with_places: false, places: HashMap::new() }
    }

    /// Definitions that find each parsed file's comments and strings as well.
    pub fn with_places() -> Self {
        Definitions { with_places: true, ..Definitions::new() }
    }

    /// The parse of PATH, or None for a file with no language: Python's (no parser). LINES reads the file's lines
    /// for the definitions' name lines.
    pub fn get(&mut self, path: &[u8], lines: &mut crate::Lines) -> Option<&Parsed> {
        if !self.files.contains_key(path) {
            let parsed = language(path).map(|lang| self.parse(path, lang, lines));
            self.files.insert(path.to_vec(), parsed);
        }
        self.files[path].as_ref()
    }

    /// Parses each of PATHS not parsed yet, on as many threads as the machine has, so that `get` finds them.
    pub fn parse_all(&mut self, paths: &[Vec<u8>]) {
        let todo: Vec<&Vec<u8>> = paths.iter().filter(|p| !self.files.contains_key(p.as_slice())).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(todo.len().max(1));
        let parsed: Vec<(Vec<(Vec<u8>, Option<Parsed>)>, HashMap<Vec<u8>, Vec<Place>>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut own = Definitions { with_places: self.with_places, ..Definitions::new() };
                        let (mut lines, mut done) = (crate::Lines::default(), Vec::new());
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(path) = todo.get(i) else { break };
                            done.push((path.to_vec(), language(path).map(|lang| own.parse(path, lang, &mut lines))));
                        }
                        (done, own.places)
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().expect("a parse does not panic")).collect()
        });
        for (done, places) in parsed {
            self.files.extend(done);
            self.places.extend(places);
        }
    }

    fn parse(&mut self, path: &[u8], lang: &'static Lang, lines: &mut crate::Lines) -> Parsed {
        let skipped = |why: &str| Parsed::Skipped(format!("ast-grep skipped: {why}"));
        // ast-grep looks up the language by the exact extension before it reads the file
        let ext = Path::new(crate::args::os(path)).extension().and_then(|e| e.to_str());
        if !ext.is_some_and(|e| lang.ast_grep_extensions.contains(&e)) {
            return skipped("no language");
        }
        let data = match std::fs::read(crate::args::os(path)) {
            Ok(d) => d,
            Err(e) => {
                return skipped(match e.kind() {
                    std::io::ErrorKind::NotFound => "notFound",
                    std::io::ErrorKind::PermissionDenied => "permissionDenied",
                    _ => "ioError",
                });
            }
        };
        // ast-grep parses the UTF-8 text, which for a Latin-1 file is the decoded copy Python's explore gives it
        let text = py::decode(&data);
        if text.len() > MAX_FILE_SIZE && text.lines().count() > MAX_LINE_COUNT {
            return skipped("fileTooLarge");
        }
        if text.is_empty() {
            return skipped("emptyFile");
        }
        let compiled = self.compiled.entry(lang.name).or_insert_with(|| compile(lang));
        let mut parser = Parser::new();
        parser.set_language(&compiled.language).expect("the grammars are built for this tree-sitter");
        let Some(tree) = parser.parse(text.as_bytes(), None) else {
            return Parsed::Skipped("ast-grep failed".into());
        };
        let file_lines = lines.insert(path, &text);
        let mut defs = Vec::new();
        let mut places = Vec::new();
        let mut columns = CharColumns::default();
        // the first line of each led definition's leads, by the definition's first byte; a lead comes before it
        let mut leads: HashMap<usize, usize> = HashMap::new();
        let mut cursor = tree.walk();
        'walk: loop {
            let node = cursor.node();
            if self.with_places
                && let Some(&what) = compiled.places.get(&node.kind_id())
            {
                let (s, e) = (node.start_position(), node.end_position());
                places.push(Place {
                    start: (s.row, columns.of(file_lines, s.row, s.column)),
                    end: (e.row, columns.of(file_lines, e.row, e.column)),
                    what,
                });
            }
            if let Some(led) = compiled.led(node) {
                let line = node.start_position().row + 1;
                leads.entry(led.start_byte()).and_modify(|l| *l = (*l).min(line)).or_insert(line);
            }
            if compiled.is_definition(node) {
                let (first, end) = (node.start_position().row + 1, node.end_position().row + 1);
                let name = compiled.name_node(node);
                let label = name.map_or(first, |n| n.start_position().row + 1);
                let sig = file_lines.get(label - 1).map_or("", |l| py::strip(l)).to_string();
                let named = name.or_else(|| compiled.type_node(node)).map(|n| &text[n.byte_range()]);
                let name = named.map(|t| name_of(lang.name, t));
                let start = leads.get(&node.start_byte()).map_or(first, |&l| l.min(first));
                defs.push((
                    (node.start_byte(), std::cmp::Reverse(node.end_byte())),
                    Def { kind: node.kind().to_string(), name, start, end, sig, line: label },
                ));
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break 'walk;
                }
            }
        }
        if self.with_places {
            self.places.insert(path.to_vec(), places);
        }
        // by lines, then bytes, then the definition's own fields, as the Python script sorts ast-grep's matches
        defs.sort_by(|(a_bytes, a), (b_bytes, b)| {
            let key = |d: &Def| (d.start, std::cmp::Reverse(d.end));
            (key(a), a_bytes, &a.sig, &a.kind, &a.name).cmp(&(key(b), b_bytes, &b.sig, &b.kind, &b.name))
        });
        Parsed::Defs(defs.into_iter().map(|(_, d)| d).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_exists_in_its_grammar() {
        for lang in LANGUAGES {
            compile(lang);
        }
    }

    #[test]
    fn languages_follow_python_extension_rules() {
        assert_eq!(language(b"a/B.JAVA").map(|l| l.name), Some("java"));
        assert_eq!(language(b"x.hxx").map(|l| l.name), Some("cpp"));
        assert!(language(b"skip/..rs").is_none());
        assert!(language(b"notes.txt").is_none());
    }
}
