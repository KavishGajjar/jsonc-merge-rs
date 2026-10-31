# Roadmap

V1 ships the documented JSONC/object/keyed-record merge behavior. Future work
should follow user needs, with distinct contracts and independently checked
algorithms. No broken implementation or undocumented restriction is reserved
for a later release.

Promising directions to investigate:

- Structural diff and move/rename inference for configuration review, with an
  explicit ambiguity/cost model rather than guessed identity during merge.
- Reference-aware configuration migrations across related documents.
- Interactive merge sessions that retain user decisions and invalidate them
  correctly after source changes.
- Recoverable multi-file application after a real workflow establishes the
  necessary failure and filesystem semantics.

Before a public launch, collect independent intended-user feedback on launch,
task and tool-configuration workflows. Improve ergonomics and examples based on
that feedback. Adoption and repository stars cannot be inferred from test counts
or feature scope.

YAML/TOML adapters, cross-file references, a GUI/LSP, a general patch algebra and
incremental parsing are outside the current release. A separate crate should be
introduced only when a real dependency/API boundary warrants it.
