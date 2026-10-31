#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PYTHONDONTWRITEBYTECODE=1
# Use the target's bundled musl runtime with rustc's default linker. Some
# distro musl-gcc wrappers inject an interpreter into otherwise static PIEs.
cargo_args=(--locked --offline --config "${CARGO_VENDOR_CONFIG:?vendored sources are required}")
evidence_dir="${EVIDENCE_DIR:-$(mktemp -d /tmp/jsonc-binary-verification.XXXXXX)}"
mkdir -p "$evidence_dir"
export EVIDENCE_DIR="$evidence_dir"
node scripts/source-hashes.cjs > "$evidence_dir/source-before.sha256"
python3 -m unittest discover -s scripts/tests -v
cargo test --workspace --release --target x86_64-unknown-linux-musl "${cargo_args[@]}" --no-fail-fast
cargo build -p jsonc-merge-cli --release --target x86_64-unknown-linux-musl "${cargo_args[@]}"
python3 scripts/release.py package --target x86_64-unknown-linux-musl \
  --binary "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/jsonc-merge" --output "$evidence_dir"
version="$(python3 scripts/release.py metadata | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')"
python3 scripts/release.py smoke "$evidence_dir/jsonc-merge-v$version-x86_64-unknown-linux-musl.tar.gz"
node scripts/source-hashes.cjs > "$evidence_dir/source-after.sha256"
diff -u "$evidence_dir/source-before.sha256" "$evidence_dir/source-after.sha256"
printf 'Offline x64 musl binary-package verification passed. Evidence: %s\n' "$evidence_dir"
