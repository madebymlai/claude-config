---
name: exploring-code
description: Explore a codebase using REPL scripts and parallel searches that locate matches within their enclosing definitions. Use to trace a feature or flow, find call sites affected by a change, or map an unfamiliar repository.
---

# Exploring code

Explore in rounds. Each round is one REPL script that runs the whole frontier in parallel under one `Promise.all` and returns only what the next decision needs. The frontier is the unread files, symbols and patterns identified by the previous result. If it points to one definition, the frontier may be a single `show`; if it points to several things, explore them together. A round costs one model round trip regardless of the number of commands.

## Search and read

Use `scripts/explore` in this skill's base directory:

- `hits` runs `rg` and groups each matching line under its owner chain (such as `impl Foo > fn new`) and the enclosing definition's line range, as parsed by `ast-grep`. Use `show FILE:START-END` to read that definition whole. `hits` also searches and tags tracked files that a plain `rg` would skip because of `.gitignore`. It accepts `rg` flags; for example, `-C 2` includes surrounding lines.
- `outline` lists a file's definitions and their ranges.
- `show FILE:LINE` reads a window around a line; pass multiple locations to read several windows in one call.
- `hits` and `outline` accept `--json` for structured results.

A JSON `hits` result has a `total` of matching lines and one `rows` entry per line. `rows` is null when the result contains counts instead: when `over` is true or when using `-l` or `-c`. Each row has `text` (the whole line), `owners` (enclosing definitions, outermost first), `ctx` (lines requested by `-A`, `-B` or `-C`), and `hidden` (whether `.gitignore` hides the file). Its `test` flag guesses whether the line is test code from its path, owners or a `#[cfg(test)] mod` declaration.

Each row's `subs` has one entry per match. `col` and `end` delimit the match in `text.slice(col, end)`; `in` is `code`, `comment`, or `string` according to the parser, or null if the file was not parsed (see `skips`); and `def` is true when the match names a definition on that line.

Define the helpers below in the first script and keep them in the REPL. `J` throws on a failed search or a result over the cap; `all` retains the failure as that command's `{error}` while returning the other results. A classifying round returns a digest of its matches, not the rows. The first round can search using these helpers without first reading `--help`:

```js
const BASE = "<this skill's base directory>", REPO = "/path/to/repo";
const X = (...a) => $`python3 -I ${BASE}/scripts/explore ${a}`.cwd(REPO).nothrow().text();
const J = async (cmd, ...a) => {
  const d = await $`python3 -I ${BASE}/scripts/explore ${cmd} --json ${a}`.cwd(REPO).json();
  if (d.over) throw new Error(`${d.total} matches past the cap of ${d.cap}; narrow with a PATH, -t or -g`);
  return d;
};
// a round's commands by name: one that fails comes back as {error: its message} and the others still arrive
const all = async (cmds) => Object.fromEntries(await Promise.all(Object.entries(cmds).map(async ([k, p]) =>
  [k, await p.catch((e) => ({error: (e.stdout || e.message).slice(0, 500)}))])));
// a match's shape: test code or not, a definition's own name, where the parse puts it, and in code the qualifier or
// word before it and the character after it
const shapeOf = (r, s) => (r.test ? "test " : "") + (s.def ? "def" : s.in !== "code" ? s.in ?? "unparsed" : (() => {
  const before = r.text.slice(0, s.col), after = r.text.slice(s.end).trimStart();
  const q = before.match(/(?:[\w$]+\s*)?(?:\.|::|->)\s*$/)?.[0].replace(/\s+/g, "") ?? before.match(/\b[\w$]+\s+$/)?.[0] ?? "";
  return `${q}▮${after[0] ?? ""}`;
})());
const matchesOf = (doc) => doc.rows.flatMap((r) => r.subs.map((s) => ({r, s, shape: shapeOf(r, s)})));
// one entry per shape: its count, files, top owners and one sample
const digest = (ms) => Object.entries(Object.groupBy(ms, (m) => m.shape)).map(([shape, g]) => ({shape, n: g.length,
  files: new Set(g.map((m) => m.r.file)).size, owners: [...new Set(g.map((m) => m.r.owners.at(-1)?.name))].slice(0, 3),
  sample: `${g[0].r.file}:${g[0].r.line}  ${g[0].r.text.trim()}`})).sort((a, b) => b.n - a.n);

const {uses, outline} = await all({uses: J("hits", "-w", "-t", "rust", "load_config"), outline: X("outline", "src/config.rs")});
if (uses.error) throw new Error(uses.error);
const matches = matchesOf(uses);
({errors: uses.errors, matches: matches.length, shapes: digest(matches), outline: preview(outline, 4000)})
```

In the next round, select the shapes that answer the question and build the listing from `matches`, which remains in the REPL:

```js
const calls = matches.filter((m) => ["config::▮(", "Self::▮("].includes(m.shape));
[...new Set(calls.map((m) => `${m.r.file}:${m.r.line}`))].join("\n")
```

## Trace callers

For a chain from a definition to the program's entry points, use `scripts/trace callers TARGET --depth N --json`. `TARGET` is `Type::method`, `Class.method` or a bare name. The trace resolves receivers from declarations, links trait methods to code that constructs their types, follows a spawn of the program itself into `main`, and lists test sites without climbing them.

Each `edges` entry has `depth`, `kind` (`call`, `call?`, `new`, `spawn`, ...), `file`, `line`, `fn` (the definition containing the site), `into` (the definition reached), `test`, `text`, and `tag` (the reason an edge is uncertain, or null when resolved). `set_aside` counts matches ruled out under each definition; `entries` lists definitions where the climb found no caller. Keep the document in the REPL, return the edges needed to answer the question, and read tagged edges with `show`.

```js
const T = (...a) => $`python3 -I ${BASE}/scripts/trace ${a}`.cwd(REPO).json();
const up = await T("callers", "Store::save", "--depth", "8", "--json");
({prod: up.edges.filter((e) => !e.test).map((e) => `${e.depth} ${e.kind} ${e.file}:${e.line} ${e.fn}${e.tag ? `  [${e.tag}]` : ""}`),
  tests: up.edges.filter((e) => e.test).length, entries: up.entries})
```

## Exploration workflow

Repeat the continuing and classification rounds as needed before finishing.

1. **Open.** Search the request's own names and domain terms, and outline the files where they appear. If it asks for every instance of a kind of thing, search each syntax form the language allows in the same round: for example, an environment variable may appear in a call, macro, attribute such as clap's `env =`, or constant. If it asks for the caller chain to entry points, run `trace callers` on the definition in that round. To find every call site of one definition, use one `hits` search and classify its matches by shape. Pass regexes as `String.raw` values so their backslashes reach `rg`.
2. **Continue.** Read the result, identify the new frontier, and explore all of it in the next round. Run named commands through `all` so one failure returns an `{error}` without losing the other results. Return only what the next decision needs: digests, owner chains, outlines, counts and `show` windows. Filter listings to the relevant entries and return the count beside each listing, so a cut cannot hide a row. Use `preview(x, N)` for text to skim; the full text remains in its REPL variable. Return a whole file only when you will read it all.
3. **Classify.** When matches need classification (for example, calls of a target, uses of a key or readers of a field), request `--json` rows scoped to the language with `-t` and return `digest(matchesOf(doc))`. This counts each `subs` entry, so two matches on one line count twice. Examine the shapes, then select the ones that answer the question in the next round from the matches retained in the REPL; check that the shape counts add up to the match count. If a sample cannot decide a shape, return its rows, capped, and `show` their owners. If the default shape does not distinguish the cases, group by a useful field instead, such as the argument following a match. A rule that only guesses from text, such as a comment marker or receiver name, puts the row in a residue rather than discarding it. A bin decided by row fields may feed another command in the same script; return the residue for inspection, with `show` of its owners when that will decide it.
4. **Finish.** Once the question is closed, build the listing in the script in the answer's format and copy it into the answer once. For a flow, cite a returned `file:line` for every hop from entry to outcome. For call sites, classify every `hits` match as a call of the target or not, and ensure the result header shows no cap. Mark conclusions inferred without reading as inferred.

For call sites, attribute each match to its definition before counting it. When another class defines the same method name, use the receiver or qualifier, owner chain and `-C` lines to resolve it. If the codebase-memory graph indexes the repository, `trace_path` with `direction: "inbound"`, `depth: 1`, `include_evidence: true` and the qualified name from `search_graph` lists direct callers with resolver and confidence. Keep `lsp` edges and edges at 0.9 confidence or above, and confirm each with `hits`. Also trace process spawns, timers, framework callbacks, channels and file hand-offs with `hits`: these link code without a call edge.
