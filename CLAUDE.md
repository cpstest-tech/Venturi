# Instructions for Claude Code

## Language

Everything in the repository (code, comments, test messages, docs, commit
messages) is written in English. The only exception is the `it:` entries of
the locale files in `crates/vv-app/locales/`, which are the Italian UI
translation.

## Comment style

Hard rule: comments in the code must be **short** and written **only when
necessary** — never to explain obvious things or things that can already be
read from the code itself (well-chosen variable/function names are enough).

A comment is justified only when it explains a non-obvious *why*: a hidden
constraint, the reason for a non-obvious choice, a worked-around bug, a
behaviour that would surprise the reader. If the code stays clear without
it, the comment must not be written.

Avoid in particular:
- Multi-paragraph essay comments on a single line or function.
- Repeating in the comment what the code already says (the *what* instead
  of the *why*).
- Long preambles/historical context when a single terse line would do.

## Tests

Unit tests do NOT go inline in the source file: they live in
`crates/<crate>/src/tests/<same path as the source>.rs`, pulled in with
`#[cfg(test)] #[path = "tests/<file>.rs"] mod tests;`. Never add an inline
`mod tests { … }`. Details in the "Where unit tests live" section of
`docs/BUILDING.md`.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. When the answer is under a Markdown heading, `graphify section "<heading>"` prints that section's text, so the .md need not be opened. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- A committed graphify-out/graph.json is rewritten whole on every build, so a large diff to it in a pull request is expected. Do not flag it as churn or suggest skipping the graph rebuild or its hooks.
