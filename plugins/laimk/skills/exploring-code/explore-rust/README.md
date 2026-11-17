# explore-rust

A Rust rewrite of [`scripts/explore`](../scripts/explore) as one binary, with the same `hits`, `outline` and `show`
commands. Its stdout and exit codes are the Python script's, byte for byte, on the regression set in
[`regress/`](regress/), except for the differences listed below. `explore-rust --help` gives the usage.

## How it works

- **Search.** `hits` searches with the crates rg is built from (`grep-searcher`, `grep-regex`, `grep-pcre2`,
  `ignore`), configured as rg 15.2 configures them for the flags the Python script passes to rg. The sink keeps
  what the script reads from rg's JSON events: per-file counts, plus match and context rows until the cap is passed.
  Files are searched as raw bytes, as rg searches them. A line that is not UTF-8 is decoded as Latin-1 only for
  display, so a non-ASCII pattern matches exactly what it matches in rg. Memory is bounded by the rows kept (at most
  the cap's worth, or under `--json` 10,000 matching lines when the cap is lower, to digest their shapes), one entry
  per matching file, and the paths the walk takes. It never grows with the search's output.
- **Tracked files `.gitignore` hides.** These are `git ls-files -z` over the scope, minus the files a walk with
  explore's own flags (`--hidden --no-ignore-dot -g !.git`) takes, which is what the Python script computes. The
  search walk records what it takes when the user's flags leave that walk unchanged. Otherwise a separate walk
  records it. The leftover files are searched at depth 1 with the user's flags, as the Python script's second rg
  does.
- **Definitions.** Each matched file is parsed with tree-sitter 0.27.2 and the grammar versions ast-grep 0.51.3
  locks, pinned with `=`. The parser looks for the same node kinds, the same name child and the same pre-order the
  script's ast-grep rule uses. A definition's range starts at the first of its leads, found as the script's `LEADS`
  rules find them: a Rust `attribute_item` whose next sibling past attributes and comments is a definition, and a
  Python `decorated_definition`. ast-grep's reasons for skipping a file map to the same labels:
  - `no language`: the exact-case extension is not one ast-grep knows, as with `.JAVA` or `.hxx`;
  - `emptyFile`;
  - `fileTooLarge`: over 3,000,000 bytes and 200,000 lines;
  - `permissionDenied`, `notFound` or `ioError` when the read fails;
  - `(ast-grep failed)`: only if tree-sitter returns no tree.
- **Python semantics.** [`src/py.rs`](src/py.rs) holds the parts of the output that follow CPython:
  - `str.strip`'s whitespace;
  - `os.path.normpath` and `relpath`;
  - surrogate-escaped names printed as `\udcXX`, and sorted by code point;
  - `Counter.most_common` tie order.

## Build

```sh
cargo build --release      # target/release/explore-rust
cargo test --release       # unit tests
```

[`.cargo/config.toml`](.cargo/config.toml) sets `PCRE2_SYS_STATIC=1`, so PCRE2 is linked in and the binary needs
only libc.

## Flags

`hits` reads its arguments as the Python script does, with the same three sets. Each refused flag gets
`explore: FLAG is not supported by hits; run rg directly for it` and exit 2, including a short flag inside a
cluster or glued to its value (`-io`, `-M10`, `-rX`). Then:

- dropped: `-n -N -R -H --with-filename --extended-regexp --line-number --no-line-number --recursive --color --colors
  --sort --sortr --sort-files --no-sort-files`;
- explore's own: `-l -c --files-with-matches --count --count-matches --max --json`;
- every other rg flag is taken, as rg 15.2 reads it.

The rg flags taken are:

- **Case and matching:** `-i -S -s -w -x -F -U --multiline-dotall --crlf --null-data --no-unicode
  --no-pcre2-unicode -v --stop-on-nonmatch`.
- **Engine:** `-P`, `--engine`, `--auto-hybrid-regex`, `--regex-size-limit`, `--dfa-size-limit`.
- **Binary, encoding and input:** `-a --binary`, `-E`/`--encoding` (`auto`, `none` or any WHATWG label),
  `--pre`/`--pre-glob`, `-z`/`--search-zip`, `--mmap`.
- **Context and limits:** `-A -B -C -m`.
- **Walk:** `-L --hidden -d/--max-depth --max-filesize --one-file-system`.
- **Ignore files:** `--no-ignore` and its `--no-ignore-*` parts, `-u`, `--no-require-git`, `--ignore-file`,
  `--no-ignore-files`, `--ignore-file-case-insensitive`.
- **Globs and types:** `-g --iglob --glob-case-insensitive -t -T --type-add --type-clear`.
- **Patterns, threads and messages:** `-e -f -j`, `--no-messages`, `--no-ignore-messages`, `--no-config`.
- **Negations:** the `--no-*` forms of those switches, and the other way round.

A flag rg does not know is refused with `explore: FLAG is not supported by explore-rust`, where the Python script's
rg reports it; both exit 2.

## JSON

`--json` on `hits` and `outline` prints one compact JSON document, built from the same values the text is printed
from. `--help` gives the schema. The structs in [`src/json.rs`](src/json.rs) declare their fields in the Python
script's key order, and serde_json's compact writer escapes as `json.dumps(separators=(",", ":"),
ensure_ascii=False)` does. A name that is not UTF-8 holds each undecodable byte as the text prints it, as the six
characters `\udcXX`, which JSON writes as `\\udcXX`. `show` refuses `--json`. A `--json` run that succeeds writes
only the document and exits 0, with `outline`'s missing FILE arguments in `missing`. Each `hits` row's `subs`
gives every match on the line in characters, and where it starts. `in` is `code`, `comment` or `string`, from
the innermost of the file's comment, string and interpolation nodes that holds it (the Python script's PLACES
table, in [`src/defs.rs`](src/defs.rs)). It is null for a file with no parse. These nodes are collected only for
`--json`, from the same parse as the definitions. Each sub's `shape` and the document's `shapes` digest follow the
script's `shape` and `shapes`, in [`src/json.rs`](src/json.rs).

`regress/run.py` compares its JSON cases with `scripts/explore` byte for byte. When the bytes differ, it also says
whether the documents are equal once decoded with msgspec. It reports any successful `--json` run, by either
implementation, that writes to stderr. It also runs both with a failing ast-grep and with none on `PATH`, and checks
that each run either succeeds with nothing on stderr and `warnings` as the document's last key, or fails with a
message and no document.

## Deliberate differences from the Python script

- **`--help`** describes this binary: tree-sitter instead of ast-grep.
- **No size guard on `in`.** The Python script skips the comments and strings of a file whose lines would make
  ast-grep print over 64 MB, such as a minified bundle, and gives its `subs` `in: null` with a warning.
  explore-rust reads those nodes from the parse it already holds, so it places such a file's matches too. No
  regression case reaches the budget.
- **`warnings` is always empty in `--json`.** The Python script lists there that ast-grep is missing or exited
  nonzero, or that a file's lines are too long to place, and without ast-grep its `outline` fails. explore-rust
  parses in process and has none of these to report.
- **Line numbers in `show`** must be ASCII digits. Python's `isdigit` and `\d` also accept other scripts' digits.

## Regression harness

```sh
python3 -I regress/run.py          # every case, text and --json: stdout and exit code against scripts/explore
python3 -I regress/run.py --keep   # also writes each case's Python output to /tmp/explore-rust-regress
python3 -I regress/tags.py         # the [tracked, hidden by .gitignore] files, compared as sets per case
python3 -I regress/timing.py       # median of 9 alternating runs and peak memory
```

`timing.py`'s memory case, `hits -l -uu ''` in helix-trader, searches every line of its `target/` and runs once
per implementation. It is the case that caught the Python script holding rg's whole output in memory.

`run.py` rebuilds the fixture repository with [`regress/make_fixture.py`](regress/make_fixture.py). The fixture
holds all 17 languages, Latin-1 contents and names, skipped and binary files, tracked files `.gitignore` hides,
long lines and nested blocks for `-C`. The other cases run in repositories and review fixtures on the author's
machine, and a case whose directory is missing is skipped.
