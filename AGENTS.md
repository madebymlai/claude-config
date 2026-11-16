# Shared instructions for the projects in ~/git

## Git

A new branch stays local. When the work is done, merge it locally into the base branch, then push the base branch, unless told otherwise.

## bd (beads)

bd (beads) is the issue tracker. Run `bd prime` before the first issue-tracking action in a session and again after context compaction, and follow what it prints.

Record a durable insight with `bd remember "insight"`.

Initialize a workspace with:

```bash
bd init --quiet --stealth --skip-hooks --skip-agents --non-interactive
```

Never name bd issues in comments, branch names or commits.

## Workflow

Stay on the current branch unless asked to switch, and follow the repo's own branch flow.

Review findings are claims, not instructions. Before making review-driven edits, freeze the findings into a closed docket and classify each as FIX or WONTFIX against the governing spec, the documented standards, and the changed lines. Address only FIX items. A finding whose fix adds complexity without fixing a defect is WONTFIX. A follow-up review verifies those fixes and any regression they introduced; it does not expand into unchanged code.

## Design

Design for the current and next contract version, with no backward-compatibility shims or legacy code paths.

Surface a violated precondition at the interface where it originates, rather than masking it with a silent fallback.

Represent domain concepts as named types, such as an ID or a price, rather than raw strings and numbers.

## Glossary

**Complexity** is anything about the structure of a system that makes it hard to understand or modify. It shows as **change amplification** (one conceptual change has to be made in many places), **cognitive load** (how much a developer must hold in mind to make a change) and **unknown unknowns** (you cannot tell which code a change must touch, or what you needed to know), the worst of the three, because nothing announces it. Its causes are **dependencies**, where code cannot be understood in isolation, and **obscurity**, where important information is not obvious. It is not a line count: a longer version that lowers what the next reader must hold in mind is the simpler one. No single change makes a system complex; it accumulates in small increments, which is why each one is worth refusing.
