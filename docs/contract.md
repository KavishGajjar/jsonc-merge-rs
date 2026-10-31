# V1 behavior contract

## Inputs and limits

An input contains exactly one UTF-8 JSON value, with optional `//` line comments,
`/* */` block comments and trailing commas. Line comments end at CR or LF; U+2028
and U+2029 inside a line comment do not end it. Outside strings/comments only JSON
whitespace (space, tab, CR, LF) is accepted. A BOM is rejected. Literal Unicode
scalars and valid paired UTF-16 escapes are accepted in strings. Unpaired
surrogates, control characters, missing commas, unquoted keys, single-quoted
strings, extended escapes, hexadecimal/non-finite numbers and other JSON5 syntax
are rejected. Parsing is complete, never a fault-tolerant partial merge.

Decoded duplicate object keys are errors, including `"a"` and `"\u0061"`.
Strings, object keys and identities are case-sensitive, without Unicode
normalization. Object order is presentation; ordinary array order is semantic.

Numbers use an exact signed decimal coefficient and arbitrary-size decimal
exponent. Zero has no sign for comparison. `1`, `1.00` and `10e-1` compare equal;
`9007199254740992` and `9007199254740993` differ. Huge exponents are never expanded.
The original selected token is emitted, not a normalized number string.

Default `Limits`:

| Resource | Default |
| --- | ---: |
| Bytes per input and rendered output | 8 MiB |
| Nested object/array containers | 128 |
| Value nodes per input/output (including containers) | 200,000 |
| Comment tokens per input/output | 200,000 |
| Bytes in one number token, including exponent | 4,096 |
| Sum of surviving record pairs considered across ordered arrays | 1,000,000 |
| Combined semantic path storage estimate across merge inputs | 32 MiB |

`Document::parse_with_limits` and `merge_with_limits` allow callers to tighten or
raise bounds, except for the hard supported depth ceiling of 128. Merge checks
all snapshots against its limits. Rendering checks the output against those same
limits. A conflict-free plan can still encounter an output budget error. Limits
return `Budget` diagnostics, never truncated results, guessed orders or conflicts.

Path storage is checked before importing snapshots or constructing report paths.
For every value node in every input, sum the cost of its complete semantic path:
each component costs 32 bytes plus its decoded property-name or declared record
identity bytes (unconfigured array indices add no text bytes). The root costs
zero. This conservative, portable estimate also covers paths below atomic arrays;
it prevents long ancestor names from multiplying into unbounded report memory.
It is not a serialized JSON report size limit. Raise `max_path_bytes` explicitly
when an application intentionally needs larger expanded paths. Comment-token
limits are independent of value-node limits and apply to document trivia too.

Policy construction additionally permits at most 200,000 declarations, 8 MiB
total path/key bytes, and 128 decoded path components. Serialized policies must
be strict JSON and satisfy the document input limits. These are explicit safety
bounds, not promises of constant memory or a particular latency. The verification
measurements exercise 5,000 object members, 998,991 ordering pairs and the 8 MiB
input boundary, plus 10,000 unordered records. Depth and small custom budget
boundaries have dedicated tests.

## Policy resolution

A policy file is `{"version":1,"arrays":[...]}`. Unknown fields, unsupported
versions, duplicate paths and declarations below another declared array fail.
Each declaration has an exact JSON Pointer `path`, string field name `key`, and
`order` of `ordered` or `unordered`. `~0` and `~1` decode to `~` and `/`; other
tilde escapes are invalid. The empty pointer targets the root. Numeric-looking
components can name object properties but never index arrays.

A missing path has no target. A present target must be an array of object
records, each with a unique string identity field. Traversing an actual array
while resolving a path fails. All three inputs are validated before equality
shortcuts. Identity values match records across branches; they do not describe
source offsets. No wildcard, normalization or implicit identity inference occurs.

## Values, objects and membership

Registered identity policy aside, equal changes converge and a change on only
one side wins. Objects merge recursively by decoded property name. Independent
properties combine, including when both branches add an object absent from the
base. Competing unequal scalar/type changes conflict. An unconfigured array is
atomic; competing unequal whole-array changes conflict, including owned comments.

Deletion wins over an unchanged member. Deletion against changed content or
owned comments conflicts. Object-property reordering alone is not a value edit.
Keyed array membership is resolved before ordering, so moving an otherwise
unchanged record does not resurrect it against deletion. The same identity added
on both branches merges as an object; incompatible shared fields still conflict.

Object presentation follows ours' surviving property order and then theirs-only
properties in their source order. This does not change semantic object equality.

## Record order

For every pair of surviving record identities, examine their relative order in
base, ours and theirs. With a base relation, a branch change supersedes an
unchanged relation; equal branch changes converge. If only one branch contains
the pair, its relation is used. If neither contains it, the base relation remains.
Without a base relation, every available branch relation must agree. A pair
never co-occurring in any input imposes no relation.

The selected precedences must all hold. A cycle, including contradictory new
pair orders, is an `Order` conflict with a closed witness. Otherwise choose the
lexicographically smallest topological order using decoded identity UTF-8 bytes.
This is deterministic and does not silently favor ours in an ordering conflict.

For `unordered`, order is not an edit: retain ours' surviving record order and
append theirs-only identities sorted by UTF-8 bytes. If ours has no such array,
all theirs' records are theirs-only. Compare unordered arrays as identity maps
when asserting semantic branch symmetry; their presentation order can differ.

## Comments and source

Ours is the presentation template; whitespace-only changes from theirs need not
be adopted. Same-line trailing comments belong to the preceding member or array
element, even if the separator comma precedes the comment. Full-line comments
before a member belong to the following member. A comment on the opening line
before the first member belongs to that first member. A comment before a closing
delimiter with no following member belongs to the container, except for a
same-line trailing comment owned by the previous member. Document header/footer
comments belong to the document. Comments between an object key and its value
belong internally to that member.

Each owned leading/internal/trailing/container/document comment sequence merges
as one three-way value using exact comment text. One-sided edits win, identical
edits converge, competing edits conflict. Comment whitespace inside its token is
part of its text; surrounding indentation is presentation. Moving records moves
their owned comments. Appending a member cannot steal an inline comment.

Unchanged documents emit exactly ours' bytes. Unchanged subtrees and original
property prefixes are reused. With stable membership/order/comment layout,
rendering replaces only changed child value spans in the original container.
When membership, order or comments change, the affected container is rebuilt:
two-space indentation by nesting level, ours' newline convention (CRLF if present,
otherwise LF), separators between entries, no new trailing comma, and a newline
before the closing delimiter. Reusable value bodies, key/colon prefixes and owned
comment fragments remain original bytes. Separators embedded in comment gaps are
adjusted outside comment tokens only. Existing inner indentation is retained,
so a moved source fragment can retain a different indentation style. Empty
containers without comments render as `{}` or `[]`. This is not global
formatting, minimum-byte diffing or bytewise branch symmetry.

Every clean render reparses and checks exact semantic values and comment
ownership against the plan. Invalid rendered output is an error, never emitted
as a successful merge.

## Plans and reports

Documents and plans own immutable snapshots and are `Send + Sync`. Merge does not
mutate any input. Conflicts identify their kind, a semantic path, the available
source alternatives and an optional ordering witness. Missing alternatives mean
deletion. Source origins identify contributing spans; they can overlap and do
not claim a one-to-one output byte map. Paths use explicit key/record/index
segments, avoiding ambiguous string-encoded record locations.

`is_clean` means no merge conflicts. A conflicted plan cannot render. `report`
returns schema version 1 inspection data. Reports are not a portable executable
patch format. `source_edits` returns zero edits for identical output or one
common-prefix/suffix-trimmed replacement. Spans are half-open UTF-8 byte offsets
against the original ours; no minimum edit count is promised.

`apply_to` first requires exact original ours bytes, then returns rendered text.
A changed external buffer yields `StalePlan` without an output. This is an
in-memory check, not filesystem compare-and-swap or crash durability. Core code
has no filesystem, network, process, clock or global mutable state.
