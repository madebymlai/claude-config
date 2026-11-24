---
name: ask-fable-second-opinion
description: Get a second opinion from Fable on a hard problem, as a design built from a self-contained brief. Use when a problem resists the obvious fix, when a design is costly to get wrong, or when the user asks for Fable's take.
---

# Ask Fable for a second opinion

Dispatch `laimk:designer` on Fable with a brief that carries the whole problem and its context. It sees none of this conversation or what you have read, so its design is only as good as the brief, and most of this skill is building that brief. A second opinion is worth its cost because it is independent: the brief states the problem and the context, and the solution is the designer's alone.

## 1. Frame the problem

Write the problem as the user would recognise it: what must become true, and the scope, meaning what may change. Done when the goal reads as a checkable outcome.

## 2. Map the code

List every fact the problem rests on: the modules involved, their callers and what those pass and use, the types and invariants, the error modes, the tests that pin current behaviour, the performance facts (hot paths, data sizes, latency and memory budgets), and the library APIs at the pinned version.

Mark each fact this session already holds from a file opened in it. When every fact is held, go straight to the brief. Otherwise dispatch `laimk:scout` for the missing ones, one per area and all in parallel, asking for the verbatim code with `path:line` rather than a summary, since the excerpt goes straight into the brief. Done when every fact on the list is backed by an excerpt or a source opened this session, and each that stayed unknown is named as unknown.

## 3. Write the brief

The brief is **neutral**: it describes what is, never what to build. A strong engineer who has never seen this repository designs from it without a single question back. It carries, in order:

- The problem, goal and scope from step 1.
- The code: verbatim excerpts with `path:line` for everything involved (signatures, the bodies whose logic matters, representative call sites), and a sentence each for the periphery.
- The constraints, and only the hard ones: a rule goes in when a design breaking it would be rejected outright, such as a contract external callers rely on or a project rule the change must obey. Quote it.
- The performance facts, with their numbers.
- The unknowns from step 2.
- The ask, verbatim:

> Design the solution. The context above is complete; your shell is for testing logic. Get the logic right first, walking every input class, ordering and failure the problem implies, with the invariants explicit. Then weigh performance on the hot path at the sizes given, and maintainability: where the next likely change lands and whether it lands in one place. Sketch at least two genuinely different approaches and keep the one that spends complexity best. Besides the proposal, report the walk, what you ran on the bench and what it showed, and the approach that lost and why.

Done when a read-through finds no candidate solution, no direction hinted by wording, and no name that is neither defined nor excerpted.

## 4. Dispatch

Run the Agent tool with `subagent_type: "laimk:designer"`, `model: "fable"`, an `effort` and the brief as the prompt. Pick `xhigh` when the logic is intricate (state machines, concurrency, numerics, many interacting cases) or the design spans several modules, and `high` otherwise. When it returns blocked, answer its questions (scouting where needed) and send the answers to the same agent with SendMessage, so it keeps the work it has done.

## 5. Weigh the design

The design is a claim. Check each assumption it names against the code you hold, and run its logic against the edge cases you know from that code. Present it to the user with your verdict: where you agree, where you differ and why, and what you verified.
