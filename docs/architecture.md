# Architecture

The workspace has two crates. `jsonc-merge` owns all semantic decisions and
rendering. `jsonc-merge-cli` handles argument parsing, file I/O, stdout and Git's
scratch-file protocol. The CLI calls the same public library API as an embedding
application.

| Module | Responsibility |
| --- | --- |
| `source` | Strict profile adapter, parser compatibility, immutable snapshots, comment ownership, spans and budgets |
| `model` | Source-backed nodes, owned trivia and exact decimal equality |
| `policy` | Versioned policy parsing, exact paths, trie validation and record identity checks |
| `reconcile` | Object/record correspondence, membership/content decisions, comment merging, provenance |
| `order` | Pairwise precedence selection, deterministic topological ordering and cycle witnesses |
| `plan` | Public review/report model, source edits and stale-buffer guard |
| `render` | Raw-fragment reuse, surgical child substitution, container rebuilding and output revalidation |

The parser adapter uses the pinned dprint `jsonc-parser` AST temporarily. It lowers
into owned immutable `Arc` trees; vendor mutable CST handles never escape. A
preflight scan limits container depth before recursive parsing and enforces JSON
whitespace. Strict string decoding rejects lone surrogates. Duplicate decoded
keys and exact decimal numbers are checked while lowering. Original source bytes
remain available through spans; no binary floating-point representation is used.

Source origins combine an input side with a snapshot-relative byte range. Node
identity is not used as cross-document record identity: declared decoded strings
provide that correspondence. All observable iteration and tie-breaking use
stable source order or sorted collections.
Document headers and footers are immutable shared slices, so cloning a snapshot
does not copy its comment strings. A preflight path-storage estimate bounds
metadata expansion before any source imports or report path allocations.

Record matching and value reconciliation precede order selection. This separates
membership conflicts from precedence cycles. An independent test oracle checks
every total order for small inputs instead of duplicating production topological
sorting. Unordered record arrays share matching/content logic but have their own
explicit presentation rule.

A merged node keeps either a reusable original fragment or an original layout
template with merged children. This avoids reparsing through a plain JSON value
and losing comments or number spellings. Changed container rendering is checked
by reparsing, comparing semantic values and comparing owned comment sequences.
Source edits are derived from the checked output, not exposed as a general patch
algebra.

Library diagnostics and merge conflicts are separate types. Budget exhaustion
is an error rather than a conflict. There is no hidden textual fallback, mutable
global cache, filesystem operation or subprocess in the engine. Report schema
versioning and exact-buffer checks keep the public surface small without
promising a stable persisted patch protocol.

V1 uses full snapshots and recursive trees with an explicit depth ceiling. It is
not an incremental editor engine. Ordered merging is O(n²) pair selection plus
sorted-map lookups and graph processing, explicitly bounded by `Limits`. New
algorithms should be introduced when a real workflow needs them, with a separate
contract and independent tests.
