---
name: exploring-code
description: Exploring a codebase from REPL scripts, with fanned-out rg searches that return each match inside its enclosing function. Use when tracing how a feature or flow works, finding every call site a change touches, or mapping an unfamiliar repository.
---

# Exploring code

Explore in rounds. A round is one REPL script that fans out the whole frontier under one `Promise.all` and returns only what the next decision needs. The frontier is what the last result points at that you have not read yet: every file, symbol and pattern when it points at several, one `show` when it points at one definition. A round costs one model round trip however many commands it holds, so a wide frontier goes in whole.

Search and read through `scripts/explore` in this skill's base directory; `--help` lists its flags. `explore hits` runs `rg` and prints each match under its owner chain (`impl Foo > fn new`) and that definition's line range, parsed by `ast-grep`, so a hit arrives with the function it lives in and `show FILE:START-END` reads that function whole. It also searches the tracked files `.gitignore` hides from a plain `rg` and tags them, and it takes `rg`'s flags: `-C 2` brings the surrounding lines in the same round. `explore outline` prints a file's definitions with their ranges, `explore show FILE:LINE` a window around a line, several windows per call. `hits` and `outline` take `--json`, which returns the same result as data (the schema is in `--help`). In a `hits` document `total` counts matching lines, and each row is one line: `text` the whole line, `owners` its enclosing definitions outermost first, `ctx` its `-A`/`-B`/`-C` lines, `hidden` whether `.gitignore` hides its file, and `subs` one entry per match on the line, with `col`, the match's 0-based character index in `text`, and `in`, which is `code`, `comment` or `string` by the parse, or null where no parser read the file (`skips` lists it). Call `.json()` on it without `.nothrow()`, so a failed search throws with explore's message.

```js
const BASE = "<this skill's base directory>", REPO = "/path/to/repo";
const X = (...a) => $`python3 -I ${BASE}/scripts/explore ${a}`.cwd(REPO).nothrow().text();
const J = (cmd, ...a) => $`python3 -I ${BASE}/scripts/explore ${cmd} --json ${a}`.cwd(REPO).json();
const [uses, outline] = await Promise.all([J("hits", "-w", "-t", "rust", "load_config"), X("outline", "src/config.rs")]);
if (uses.over || uses.errors.length) throw new Error(`${uses.total} lines, errors: ${uses.errors}`);
// a match's shape: where the parse puts it, and in code the qualifier or word before it and the character after it
const shapeOf = (r, s) => s.in !== "code" ? s.in ?? "unparsed" : (() => {
  const before = r.text.slice(0, s.col), after = r.text.slice(s.end).trimStart();
  const q = before.match(/[\w$]+\s*(?:\.|::|->)\s*$/)?.[0].replace(/\s+/g, "") ?? before.match(/\b[\w$]+\s+$/)?.[0] ?? "";
  return `${q}▮${after[0] ?? ""}`;
})();
const matches = uses.rows.flatMap((r) => r.subs.map((s) => ({r, s, shape: shapeOf(r, s)})));
const shapes = Object.entries(Object.groupBy(matches, (m) => m.shape)).map(([shape, ms]) => ({shape, n: ms.length,
  files: new Set(ms.map((m) => m.r.file)).size, owners: [...new Set(ms.map((m) => m.r.owners.at(-1)?.name))].slice(0, 3),
  sample: `${ms[0].r.file}:${ms[0].r.line}  ${ms[0].r.text.trim()}`})).sort((a, b) => b.n - a.n);
({matches: matches.length, shapes, outline: preview(outline, 4000)})
```

The next round names the shapes that answer the question and lists them from `matches`, still in the REPL:

```js
const calls = matches.filter((m) => ["config::▮(", "Self::▮("].includes(m.shape));
Object.entries(Object.groupBy(calls, (m) => m.r.file)).map(([f, ms]) => `${f}: ${ms.map((m) => m.r.line).join(", ")}`)
```

1. **Open.** The first round searches the request's own words (names, domain terms) and outlines the files they land in.
2. **Rounds.** Read the last result, write down the new frontier, and send all of it as the next round. When a round's matches need classifying (calls of a target, uses of a key, readers of a field), take them as `--json` rows and return their shapes before any rows: one entry per match (each entry of a row's `subs`, so a line with two calls counts two), keyed by `in` and, in code, by the text around the match, each shape with its count, files, top owners and one sample. Read the shapes, name the ones that answer the question, and select them in the next round from the matches still held in the REPL, checking that the shapes add up to the matches. A shape its sample cannot decide comes back as its rows, capped, with a `show` of their owners. When the default key does not split the question, key on what does, such as the argument after the match when the call itself is always the same. Return the final listing once, in the answer's format. A rule that guesses from text alone (a comment marker, a receiver's name) sends its row to the residue rather than dropping it. A bin decided by the rows' fields can feed the next command in the same script; the residue comes back to you, with a `show` of its owners when reading will decide it. Return digests: owner chains, outlines, counts, `show` windows, each capped with `preview(x, N)`; the whole text stays in its variable for the next round. Bring a whole file back only when you will read all of it. `.nothrow()` on every text command keeps one failed path from costing the round the rest.
3. **Finish** when the question is closed. For a flow, every hop from entry to outcome cites a `file:line` a round returned. For call sites, every match `hits` reported is classified as a call of the target or not, and its header shows no cap. Mark what you inferred without reading as inferred.

For call sites, attribute each match to its definition before counting it: a method name another class also defines resolves by its receiver or qualifier, which the owner chain and `-C` lines show. When the codebase-memory graph indexes the repository, `trace_path` with `direction: "inbound"`, `depth: 1`, `include_evidence: true` and the qualified name from `search_graph` lists direct callers with their resolver and confidence. Keep the `lsp` edges and those at 0.9 and above, and confirm each with `hits`. A process spawn, a timer, a framework callback, a channel or a file hand-off links code with no call edge, so trace those links with `hits`.
