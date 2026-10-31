# JSONC Merge

A Rust library and command-line tool for three-way merges of commented JSON
configuration. Give named records an explicit identity and merge their edits
across reorders, without discarding their comments.

For example, one branch can move a launch configuration while another changes its
program. The result includes both changes. Competing values, deletion against an
edit, incompatible comment changes and impossible record orders produce a
structured conflict instead of a silent winner.

**v0.1.0, pre-release.** Build from source with the instructions below. This
workspace has not been released to crates.io.

## Try it

Requires Rust **1.95.0**. Git is required for the Git adapter tests; Node.js is
required only for the independent development oracle. The Rust library and CLI
do not require Node.js or network access at runtime.

```sh
cargo build --release --locked
target/release/jsonc-merge merge \
  examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc \
  --policy examples/launch/policy.json
```

The [launch example](examples/launch/README.md) keeps a reordered worker before the
API and changes the API's program, with the comments still attached to their
records. Install the CLI from this local checkout with:

```sh
cargo install --path crates/cli --locked
```

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
