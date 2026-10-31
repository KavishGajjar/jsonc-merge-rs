# JSONC Merge

A Rust library and command-line tool for three-way merges of commented JSON
configuration. Give named records an explicit identity and merge their edits
across reorders, without discarding their comments.

[![verify](https://github.com/KavishGajjar/jsonc-merge-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/KavishGajjar/jsonc-merge-rs/actions/workflows/ci.yml)

## Reorder on one branch. Edit on the other.

One branch moves **Worker** before **API**. Another changes the API program.
Declare `name` as the record identity and keep both edits, with each comment
still attached to its record:

| Input | Configuration order | API program |
| --- | --- | --- |
| Base | API → Worker | `src/api.js` |
| Ours | Worker → API | `src/api.js` |
| Theirs | API → Worker | `dist/api.js` |
| **Merged** | **Worker → API** | **`dist/api.js`** |

<!-- demo-output:start -->
```jsonc
// Project launch configurations. Names are the explicit record identities.
{
  "version": "0.2.0",
  "configurations": [
    // Run the background worker.
    { "name": "Worker", "type": "node", "request": "launch", "program": "src/worker.js" },
    // Start the API locally.
    { "name": "API", "type": "node", "request": "launch", "program": "dist/api.js" }
  ]
}
```
<!-- demo-output:end -->

These are actual [example inputs](examples/launch/README.md), with a
[checked expected output](examples/launch/expected.jsonc). Conflicting values,
deletions against edits, incompatible comment changes and impossible record
orders produce a structured conflict instead of a silent winner.

## Download and try

[Releases](https://github.com/KavishGajjar/jsonc-merge-rs/releases) are previews.
**v0.1.0 is source-only.** Binary releases starting with **v0.1.1** package the
executable, demo and notices for these targets after all native checks pass:

| Platform | Archive target | Format |
| --- | --- | --- |
| Linux x64, static musl | `x86_64-unknown-linux-musl` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| Windows x64, static CRT | `x86_64-pc-windows-msvc` | `.zip` |

Download your archive and `SHA256SUMS` from the same release. Follow the
[checksum and extraction instructions](docs/install.md), then run the example
from the extracted directory:

```sh
./jsonc-merge merge examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc --policy examples/launch/policy.json
```

In Windows PowerShell use `.\jsonc-merge.exe` in place of `./jsonc-merge`.
No Rust, Python or Node.js installation is needed for a packaged executable.
Git is required only for optional Git-driver integration. macOS/Windows downloads
are not distributor-signed or notarized; see the installation guide.

## Build from source

Requires Rust **1.95.0**. From a new directory:

```sh
git clone https://github.com/KavishGajjar/jsonc-merge-rs.git
cd jsonc-merge-rs
cargo install --path crates/cli --locked
jsonc-merge --version
jsonc-merge merge examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc --policy examples/launch/policy.json
```

The workspace has not been released to crates.io. Node.js and Python are used
only by development checks, not by the Rust library or CLI at runtime.

## Declare record identities

```json
{
  "version": 1,
  "arrays": [
    { "path": "/configurations", "key": "name", "order": "ordered" }
  ]
}
```

Paths are exact JSON Pointers through object properties. Identity fields must be
strings and unique within each input array. `ordered` reconciles relative order;
`unordered` merges by identity while preserving ours' surviving order, then
appending theirs-only identities in bytewise lexical order. Arrays without a
policy are atomic; competing unequal changes conflict.

No identity is guessed from a field named `id` or `name`. Renaming an identity is
treated as deletion plus addition. Policies cannot traverse array positions.

## Library

```rust
use jsonc_merge::{Document, Policy, merge};

fn main() -> Result<(), jsonc_merge::Diagnostic> {
    let base = Document::parse(r#"{"port": 8000, "debug": false}"#)?;
    let ours = Document::parse(r#"{"port": 9000, "debug": false}"#)?;
    let theirs = Document::parse(r#"{"port": 8000, "debug": true}"#)?;
    let plan = merge(&base, &ours, &theirs, &Policy::default())?;

    if plan.is_clean() {
        let jsonc = plan.render()?;
        // Or plan.apply_to(current_buffer), which checks exact original bytes.
        println!("{jsonc}");
    } else {
        eprintln!("{:?}", plan.conflicts());
    }
    Ok(())
}
```

The library retains immutable snapshots. Plans expose conflicts, source origins,
and UTF-8 source edits. Reports have a schema version; they are inspection data,
not saved executable patch programs.

## Commands

| Command | Output |
| --- | --- |
| `check FILE... [--policy FILE]` | Validate syntax and policy targets |
| `plan BASE OURS THEIRS [--policy FILE]` | JSON conflict/provenance report |
| `merge BASE OURS THEIRS [--policy FILE]` | Clean JSONC on stdout |
| `git-driver BASE OURS THEIRS [WIDTH] [--policy FILE]` | Replace Git's OURS scratch file |

Normal commands leave inputs untouched. Exit codes are **0** for success, **1**
for merge conflicts, **2** for input, policy, resource or operational errors.
`merge` emits no stdout on a conflict or input error. Git conflicts use honest
whole-file diff3 markers; see [Git setup](docs/git-driver.md).
Redirect stdout to a different file: redirecting onto an input would let the
shell truncate that input before this program starts.
Operational/input errors are JSON on stderr with `error.code`, `message`, `file`,
`span` and `input` fields. Locations use UTF-8 byte offsets; unavailable fields are
null. Merge conflicts use the versioned plan report instead.
Non-UTF-8 filenames use their lossy display form in diagnostics; source contents
must still be valid UTF-8.

## Source and format contract

JSON plus `//` and `/* */` comments and trailing commas is accepted. Duplicate
decoded keys, JSON5 extensions and isolated surrogate escapes are rejected.
Numbers compare exactly, including integers beyond floating-point precision and
huge decimal exponents, while their chosen original spelling is retained.

Ours supplies presentation. Unchanged documents round-trip byte for byte. Value
changes reuse the surrounding source where possible. Structural changes rebuild
the affected container's separators and indentation, preserving reusable member
bodies and owned comment text. This is not a formatter or a minimum-edit diff.
The [full contract](docs/contract.md) defines comment ownership, merge/order rules,
formatting boundaries and explicit resource limits.

## Verify offline in Docker

Image setup downloads the pinned toolchain and dependencies. The verification run
has no network, runs as an unprivileged user and cannot write the source checkout:

```sh
docker build -t jsonc-merge-v1 .
docker run --rm --network none --read-only \
  --tmpfs /tmp:rw,exec,size=2g,mode=1777 jsonc-merge-v1
```

This runs formatting, strict Clippy, Rust tests and doctests, documentation build,
release build, an independent JSONC output parser and informational measurements.
Rust tests include exhaustive small record-order models, a separate generated
JSON merge reference, source-preservation cases and actual disposable Git merges.
See [verification](docs/verification.md) for evidence and reproducibility details.

## Boundaries and contributing

V1 covers one JSONC document per merge. It does not infer renames, align ordinary
arrays, support YAML/TOML, resolve cross-file references, or apply multi-file
changes. See [architecture](docs/architecture.md), [contributing](CONTRIBUTING.md)
and the [roadmap](docs/roadmap.md). The source is MIT licensed; dependencies retain
their own licenses, listed in [third-party notices](THIRD_PARTY.md).
