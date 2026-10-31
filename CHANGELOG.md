# Changelog

## 0.1.1 — unreleased binary preview

- Native binary packaging for Linux x64 musl, macOS Apple Silicon/Intel and
  Windows x64 with static CRT, including checksums, examples and license notices.
- Per-target release tests, executable architecture/dependency inspection and
  smoke tests of the extracted archive before a draft pre-release is created.
- Build-only previews on branches and pull requests; tag pushes create drafts
  for manual owner publication. Existing published releases are not overwritten.
- A checked README demo and complete binary/source installation instructions.
- Upgrade checkout to its Node 24-based action revision.

## 0.1.0 — 2026-10-31 (pre-release)

- Immutable strict-profile JSONC documents, source spans and exact decimal values.
- Three-way object merging, explicit ordered/unordered string-keyed record arrays,
  conservative conflicts, comment ownership and source-fragment reuse.
- Reviewable conflict/provenance reports, checked rendering and stale-buffer guard.
- CLI validation/report/merge commands and optional repository-local Git driver.
- Deterministic tests, independent order and JSON merge oracles, independent JSONC
  output parsing, documented limits and offline Docker verification.
- Maintainer review: bounded semantic-path/comment metadata, shared document
  trivia, efficient unordered membership, file-aware CLI diagnostics, independent
  generated comment checks, a portable verification manifest and pinned CI actions.
