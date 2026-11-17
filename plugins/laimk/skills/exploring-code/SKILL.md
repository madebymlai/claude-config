---
name: exploring-code
description: Exploring a codebase from REPL scripts, with fanned-out rg searches that return each match inside its enclosing function. Use when tracing how a feature or flow works, finding every call site a change touches, or mapping an unfamiliar repository.
---

# Exploring code

Explore in rounds. A round is one REPL script that fans out the whole frontier under one `Promise.all` and returns only what the next decision needs. The frontier is what the last result points at that you have not read yet: every file, symbol and pattern when it points at several, one `show` when it points at one definition. A round costs one model round trip however many commands it holds, so a wide frontier goes in whole.

Search and read through `scripts/explore` in this skill's base directory. `explore hits` runs `rg` and prints each match under its owner chain (`impl Foo > fn new`) and that definition's line range, parsed by `ast-grep`, so a hit arrives with the function it lives in and `show FILE:START-END` reads that function whole. It also searches the tracked files `.gitignore` hides from a plain `rg` and tags them, and it takes `rg`'s flags: `-C 2` brings the surrounding lines in the same round. `explore outline` prints a file's definitions with their ranges, `explore show FILE:LINE` a window around a line, several windows per call. `hits` and `outline` take `--json`, which returns the same result as data. This section and the example below hold every field and helper a round needs, so the first round searches rather than reading `--help`. In a `hits` document `total` counts matching lines, and `rows` holds one entry per line, or null when the result is counts instead: past the cap (`over` is true) and under `-l` or `-c`. A row has `text`, the whole line, `owners`, its enclosing definitions outermost first, `test`, whether its path, its owners or a `#[cfg(test)] mod` declaration mark it as test code (a guess from names and declarations), `ctx`, its `-A`/`-B`/`-C` lines, `hidden`, whether `.gitignore` hides its file, and `subs`, one entry per match on the line: `col` and `end`, so `text.slice(col, end)` is the match, `in`, which is `code`, `comment` or `string` by the parse, or null where no parser read the file (`skips` lists it), and `def`, true when the match is the name of a definition on that line. `J` in the example throws with explore's message when a search fails and with the count when it passes the cap, and `all` keeps either as that command's `{error}`.

The helpers go in the first script and stay in the REPL; a classifying round returns the digest of its matches, never the rows:

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

The next round names the shapes that answer the question and builds the answer's listing from `matches`, still in the REPL:

```js
const calls = matches.filter((m) => ["config::▮(", "Self::▮("].includes(m.shape));
[...new Set(calls.map((m) => `${m.r.file}:${m.r.line}`))].join("\n")
```

1. **Open.** The first round searches the request's own words (names, domain terms) and outlines the files they land in. When the request names a kind of thing (every environment variable read, every route), the same round searches each syntax form that kind takes in the language: the call, the macro, an attribute such as clap's `env =`, a constant holding the name. For callers of a definition, it also searches where the chain must end and the links no call edge shows, the entry points and process spawns, as one more command of the same `all`. In Rust that is ``ends: J("hits", "-t", "rust", "-e", String.raw`fn main\b`, "-e", String.raw`Command::new\(`)``, with the CLI's command enum and `current_exe` beside them; a regex goes in as a value written with `String.raw`, so its backslashes reach `rg`.
2. **Rounds.** Read the last result, write down the new frontier, and send all of it as the next round. When a round's matches need classifying (calls of a target, uses of a key, readers of a field), take them as `--json` rows scoped to the language with `-t` and return `digest(matchesOf(doc))`: one entry per match (each entry of a row's `subs`, so a line with two calls counts two). Read the shapes, name the ones that answer the question, and select them in the next round from the matches still held in the REPL, checking that the shapes add up to the matches. A shape its sample cannot decide comes back as its rows, capped, with a `show` of their owners. When the default key does not split the question, key on what does, such as the argument after the match when the call itself is always the same. A rule that guesses from text alone (a comment marker, a receiver's name) sends its row to the residue rather than dropping it. A bin decided by the rows' fields can feed the next command in the same script; the residue comes back to you, with a `show` of its owners when reading will decide it. Return digests: owner chains, outlines, counts, `show` windows. Cap a listing by filtering it to what the question needs (the outline lines whose range holds a hit, the rows of the named shapes) and return its count beside it, so a cut never hides a row; `preview(x, N)` is for text you only skim, and the whole text stays in its variable for the next round. Bring a whole file back only when you will read all of it. Send a round's commands through `all` under names, so one that fails returns its `{error}` and the round keeps the rest.
3. **Finish** when the question is closed, with the listing built in the script, in the answer's format, and copied into the answer once. For a flow, every hop from entry to outcome cites a `file:line` a round returned. For call sites, every match `hits` reported is classified as a call of the target or not, and its header shows no cap. Mark what you inferred without reading as inferred.

For call sites, attribute each match to its definition before counting it: a method name another class also defines resolves by its receiver or qualifier, which the owner chain and `-C` lines show. When the codebase-memory graph indexes the repository, `trace_path` with `direction: "inbound"`, `depth: 1`, `include_evidence: true` and the qualified name from `search_graph` lists direct callers with their resolver and confidence. Keep the `lsp` edges and those at 0.9 and above, and confirm each with `hits`. A process spawn, a timer, a framework callback, a channel or a file hand-off links code with no call edge, so trace those links with `hits`.
