---
name: exploring-code
description: Explore a codebase using REPL scripts and parallel searches that locate matches within their enclosing definitions. Use to trace a feature or flow, find call sites affected by a change, or map an unfamiliar repository.
---

# Exploring code

Explore in rounds. Each round is one REPL script that runs the whole frontier in parallel under one `Promise.all` and returns only what the next decision needs. The frontier is the unread files, symbols and patterns identified by the previous result. If it points to one definition, the frontier may be a single `show`; if it points to several things, explore them together. A round costs one model round trip regardless of the number of commands.

## Search and read

Use `scripts/explore` in this skill's base directory:

- `hits` runs `rg` and groups each matching line under its owner chain (such as `impl Foo > fn new`) and the enclosing definition's line range, as parsed by `ast-grep`. A definition's range includes the attributes and decorators above it. Use `show FILE:START-END` to read that definition whole. `hits` also searches and tags tracked files that a plain `rg` would skip because of `.gitignore`. It accepts `rg` flags; for example, `-C 2` includes surrounding lines.
- `outline` lists a file's definitions and their ranges.
- `show FILE:LINE` reads a window around a line; pass multiple locations to read several windows in one call.
- `hits` and `outline` accept `--json` for structured results.

A JSON `hits` result has a `total` of matching lines and one `rows` entry per line. `rows` is null when the result contains counts instead: when `over` is true or when using `-l` or `-c`. Each row has `text` (the whole line), `owners` (enclosing definitions, outermost first), `ctx` (lines requested by `-A`, `-B` or `-C`), and `hidden` (whether `.gitignore` hides the file). Its `test` flag guesses whether the line is test code from its path, owners or a `#[cfg(test)] mod` declaration.

Each row's `subs` has one entry per match. `col` and `end` delimit the match in `text.slice(col, end)`; `in` is `code`, `comment`, or `string` according to the parser, or null if the file was not parsed (see `skips`); `def` is true when the match names a definition on that line; and `shape` classifies the match: `def`, `comment`, `string`, or in code the qualifier or word before the match, `▮` for the match, and the character after it, as in `config::▮(` or `s.▮(`, with `test ` in front on a test row.

The result's `shapes` digests the matches, most common shape first: each entry has the `shape`, its count `n`, its number of `files`, up to three innermost `owners` and a `sample` line. Over the cap, `shapes` still covers every match while `rows` is null, so the digest shows where the matches are before you narrow with a PATH, `-t` or `-g`, or raise `--max`. Past 10,000 matching lines `shapes` is null too.

Define the helpers below in the first script and keep them in the REPL. `J` throws on a failed search; `all` retains the failure as that command's `{error}` while returning the other results. A classifying round returns the `shapes` digest, not the rows. The first round can search using these helpers without first reading `--help`:

```js
const BASE = "<this skill's base directory>", REPO = "/path/to/repo";
const X = (...a) => $`python3 -I ${BASE}/scripts/explore ${a}`.cwd(REPO).nothrow().text();
const J = (cmd, ...a) => $`python3 -I ${BASE}/scripts/explore ${cmd} --json ${a}`.cwd(REPO).json();
// a round's commands by name: one that fails comes back as {error: its message} and the others still arrive
const all = async (cmds) => Object.fromEntries(await Promise.all(Object.entries(cmds).map(async ([k, p]) =>
  [k, await Promise.resolve(p).catch((e) => ({error: (e.stdout || e.message).slice(0, 500)}))])));

const {uses, outline} = await all({uses: J("hits", "-w", "-t", "rust", "load_config"), outline: X("outline", "src/config.rs")});
if (uses.error) throw new Error(uses.error);
({total: uses.total, over: uses.over, errors: uses.errors, shapes: uses.shapes, outline: preview(outline, 4000)})
```

In the next round, select the shapes that answer the question and build the listing from `uses.rows`, which remains in the REPL:

```js
const pick = new Set(["config::▮(", "Self::▮("]);
const calls = uses.rows.flatMap((r) => r.subs.filter((s) => pick.has(s.shape)).map(() => `${r.file}:${r.line}`));
[...new Set(calls)].join("\n")
```

## Trace callers and references

`scripts/trace` reads Rust and Java code through a cached index of definitions and call sites:

- `trace callers TARGET --depth N --budget CHARS` prints the chain from a definition up to the program's entry points as an indented tree, one call site per line. `TARGET` is `Type::method`, `Class.method` or a bare name. The trace resolves receivers from declarations, links trait methods to code that constructs their types, follows a spawn of the program itself into `main`, and counts test sites without climbing them. `[?...]` marks an uncertain edge and `[via X]` a call through the supertype `X`. Read the chain from this text, and `show` the `[?...]` edges.
- `trace refs PATTERN --budget CHARS` lists the `rg` matches of a pattern with the definition holding each, setting aside test code, comments and imports with their counts. Use it for the readers of a value, such as `env::var|var_os`.
- `trace flow TARGET --depth N` reduces a function's body to the lines that call other functions in the repository or hand work off (spawns, channels, listeners), following each call `N` levels down. Use it for what a function does; it leaves out the logic between calls, so read conditions with `show`.

To build a listing from a caller chain, run `trace callers` with `--json` and keep the document in a REPL variable. Each `edges` entry has `depth`, `kind` (`call`, `call?`, `new`, `spawn`, ...), `file`, `line`, `fn` (the definition containing the site), `into` (`{name, file, line}` of the definition reached), `test`, `text`, `tag` (why a `call?` edge is uncertain; null on every other edge) and `note` (what a resolved link is, such as the supertype a call goes through or the type constructed). `set_aside` counts matches ruled out under each definition; `entries` lists definitions where the climb found no caller.

```js
const T = (...a) => $`python3 -I ${BASE}/scripts/trace ${a}`.cwd(REPO).nothrow().text();
const chain = await T("callers", "Store::save", "--depth", "8", "--budget", "8000");
const up = JSON.parse(await T("callers", "Store::save", "--depth", "8", "--json"));
({chain, tests: up.edges.filter((e) => e.test).map((e) => `${e.file}:${e.line} ${e.fn} -> ${e.into.name}`)})
```

## Exploration workflow

Repeat the continuing and classification rounds as needed before finishing.

1. **Open.** Search the request's own names and domain terms, and outline the files where they appear. If it asks for every instance of a kind of thing, search each syntax form the language allows in the same round: for example, an environment variable may appear in a call, macro, attribute such as clap's `env =`, or constant. If it asks for every value a reader receives, such as every environment variable production code reads, search non-test code in that round for both ends: the reader forms (with `trace refs` or `hits`) and the lexical form of the values themselves, such as upper-case string literals or constants. Then join the two: a reader whose argument is a name leads to its values through the value search. If it asks for the caller chain to entry points, run `trace callers` on the definition in that round and read its text. To find every call site of one definition, use one `hits` search and classify its matches by shape. Pass regexes as `String.raw` values so their backslashes reach `rg`.
2. **Continue.** Read the result, identify the new frontier, and explore all of it in the next round. Run named commands through `all` so one failure returns an `{error}` without losing the other results. Return only what the next decision needs: digests, owner chains, outlines, counts and `show` windows. Filter listings to the relevant entries and return the count beside each listing, so a cut cannot hide a row. Use `preview(x, N)` for text to skim; the full text remains in its REPL variable. Return a whole file only when you will read it all.
3. **Classify.** When matches need classification (for example, calls of a target, uses of a key or readers of a field), request `--json` rows scoped to the language with `-t` and return the result's `shapes`. A shape counts each `subs` entry, so two matches on one line count twice. Examine the shapes, then select the ones that answer the question in the next round from the rows retained in the REPL. If the result is over the cap, narrow it to the files or directories the shapes point to before selecting. If a sample cannot decide a shape, return its rows, capped, and `show` their owners. If the default shape does not distinguish the cases, group by a useful field instead, such as the argument following a match. A rule that only guesses from text, such as a comment marker or receiver name, puts the row in a residue rather than discarding it. A bin decided by row fields may feed another command in the same script; return the residue for inspection, with `show` of its owners when that will decide it.
4. **Finish.** Once the question is closed, build the listing in the script in the answer's format and copy it into the answer once. For a flow, cite a returned `file:line` for every hop from entry to outcome. For call sites, classify every `hits` match as a call of the target or not, and ensure the result is not over the cap. Mark conclusions inferred without reading as inferred.

For call sites, attribute each match to its definition before counting it. When another class defines the same method name, use the receiver or qualifier, owner chain and `-C` lines to resolve it. Also trace process spawns, timers, framework callbacks, channels and file hand-offs with `hits`: these link code without a call edge.
