# Verification

The reproducible baseline is `Dockerfile` plus `scripts/verify.sh`. Image setup
downloads pinned Rust dependencies into a vendor directory and installs the
independent Microsoft JSONC parser from its npm lockfile. Verification runs with
`--network none`, a read-only root filesystem and user `65534:65534`. Only `/tmp`
and an optional evidence directory are writable. No daemon, external service,
global Git configuration or host Rust installation is required.

The script checks formatting, treats Clippy and Rustdoc warnings as errors, runs
all Rust tests and doctests without fail-fast, builds release binaries, verifies
dependency license notices, cross-parses clean output independently, and records
informational measurements. A before/after SHA-256 manifest verifies that source
files were unchanged, excluding generated build/evidence directories. The manifest
uses Node.js hashing rather than GNU-only utilities. Real framework failures and
assertions remain visible.

## Coverage

| Area | Independent checks and regressions |
| --- | --- |
| JSONC profile | JSON/JSONC syntax, whitespace extensions, surrogate escapes, duplicate decoded keys, comments, Unicode and CR/LF locations |
| Exact numbers | Large integers, equivalent decimal spellings, negative zero, large exponents and token limits |
| Policy | Escaped/root/object paths, invalid array traversal, duplicate declarations/identities, missing/non-string keys, validation before shortcuts |
| Ordinary merge | 2,000 seeded inputs against a separate JSON-only reference, semantic branch symmetry and one-sided identity law |
| Ordered records | 576 four-record reorder combinations and 4,096 insertion/deletion/order combinations, checked by enumerating all legal total orders |
| Record contents | Reorder plus field edit, concurrent same-ID additions, deletion versus edit, membership before ordering |
| Unordered records | Ours' presentation order, sorted theirs-only insertions, missing arrays, parent deletion/type changes and Unicode identities |
| Comments/source | Byte-exact round trips, numeric-token-only changes, leading/internal/trailing/container/document trivia, moved comments, separator gaps, CRLF and a 128-case comment matrix |
| Generated JSONC | 1,000 arbitrary one-sided pairs in both branch directions, plus 500 keyed-record changes, with mixed LF/CRLF/CR and comment-looking strings |
| Plans | Structured alternatives, actual comment spans, provenance ranges, UTF-8 source edits, deterministic reports, stale-buffer rejection and immutable snapshots |
| Budgets | Small exact custom boundaries, pre-parse node allocation guards, comment-token counts, expanded semantic paths, snapshot/output/pair limits, maximum supported depth and large flat documents |
| CLI/Git | Status categories, empty stdout on failure, no ordinary input writes, scratch-file replacement, symlink refusal and actual clean/conflicted Git merges |
| Output parsing | 259 successful outputs checked with Microsoft `jsonc-parser` 3.3.1; the additional 128 generated cases also compare independently lexed comment tokens |
| Installation/docs | Locked offline CLI installation, installed-binary version check, and README library example compiled as a doctest |
| Binary releases | Version/tag guards, four archive formats/targets, architecture/dependency checks, manifests/checksums, traversal/link rejection, draft-only API writes and idempotent retries; native extracted-binary smoke checks run in the release matrix |

The JavaScript parser is not used as a number-equality oracle beyond safe finite
numbers; exact large-number behavior has Rust fixtures. The order oracle
enumerates total orders rather than reproducing production graph sorting. The
comment matrix uses that oracle and explicit deletion/edit expectations; it does
not silently skip arbitrary conflicts.
Generated inputs use a fixed deterministic generator. No test asserts a timing
threshold or requires a network connection.

## Reproduce and retain evidence

```sh
docker build -t jsonc-merge-v1 .
docker run --rm --network none --read-only \
  --tmpfs /tmp:rw,exec,size=2g,mode=1777 jsonc-merge-v1
```

The default evidence directory is inside the temporary filesystem and disappears
with the container. To retain manifests and measurements, mount a separate empty
directory writable by UID 65534 and set `EVIDENCE_DIR=/evidence`:

```sh
docker run --rm --network none --read-only \
  --tmpfs /tmp:rw,exec,size=2g,mode=1777 \
  -v /absolute/path/to/evidence:/evidence \
  -e EVIDENCE_DIR=/evidence jsonc-merge-v1
```

Capture container stdout/stderr separately for the complete test log. Measurements
cover 5,000 object members, 998,991 ordered record pairs, 10,000 unordered records
and three equal snapshots at the 8 MiB boundary. They exercise the default bounds,
not a universal
latency or memory guarantee. Re-measure on the intended deployment hardware.

## Practical limits of this evidence

Source CI for the v0.1.0 line passed Linux, macOS and Windows on
[commit 5acc996](https://github.com/KavishGajjar/jsonc-merge-rs/actions/runs/41216318045).
New binary-release changes require their own four-target hosted preview before
tagging. Local Docker checks can exercise the Linux archive, not run Mac/Windows
executables. Follow the [release procedure](releasing.md).
There has been no independent user pilot, exhaustive proof, long-running fuzzing
campaign or production deployment. Tests reduce risk; they cannot establish an
absence of every possible defect. Publication and adoption remain separate work.
