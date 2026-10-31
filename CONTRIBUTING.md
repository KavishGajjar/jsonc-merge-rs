# Contributing

Start with the [behavior contract](docs/contract.md). A change to merge outcomes,
comment ownership, ordering or formatting boundaries needs a clear public
example and tests. Preserve conservative conflict reporting; don't add guessed
identities, implicit renames or silent textual fallbacks.

Use Rust 1.95.0. Keep dependency updates separate from behavior changes and commit
the Cargo lockfile. The original source is MIT licensed. New dependencies need a
license inventory update. The core must remain free of filesystem, network,
clock, subprocess and global mutable state.

After dependencies are available, basic Rust checks need no network:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline --no-fail-fast
```

For the independent development parser locally, install its pinned package once:

```sh
npm ci --prefix tools/jsonc-oracle --ignore-scripts --no-audit --no-fund
NODE_PATH="$PWD/tools/jsonc-oracle/node_modules" bash scripts/verify.sh
```

The Docker workflow in the README is the complete, reproducible offline baseline.
Network is used only while preparing the image, not while verifying. The source
tree can be read-only. All temporary Git repositories and generated fixtures go
in the operating system temporary directory.

Tests must be deterministic, preserve real assertion diagnostics, and cover
contract behavior rather than private implementation choices. Add a minimal
regression for each bug. Prefer an independently expressed small oracle for a
new algorithm; a second copy of the production algorithm is weak evidence.
Timing measurements are informational, never flaky pass/fail thresholds.

Current automated checks are not a substitute for review, fuzzing or real-user
feedback. CI is provided for Linux/macOS/Windows Rust checks and Linux Docker
verification; a platform is not claimed as locally tested just because a job is
configured for it.
