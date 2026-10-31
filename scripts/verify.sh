#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

evidence_dir="${EVIDENCE_DIR:-$(mktemp -d /tmp/jsonc-merge-verification.XXXXXX)}"
mkdir -p "$evidence_dir"
export EVIDENCE_DIR="$evidence_dir"
export PYTHONDONTWRITEBYTECODE=1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
cargo_args=(--locked --offline)
if [[ -n "${CARGO_VENDOR_CONFIG:-}" ]]; then
  cargo_args+=(--config "$CARGO_VENDOR_CONFIG")
fi
source_hashes() {
  node scripts/source-hashes.cjs
}
source_hashes > "$evidence_dir/source-before.sha256"
rustc --version
cargo --version
git --version
node --version
node scripts/source-hashes.test.cjs
python3 -m unittest discover -s scripts/tests -v
cargo fmt --all -- --check
cargo clippy --workspace --all-targets "${cargo_args[@]}" -- -D warnings
cargo test --workspace "${cargo_args[@]}" --no-fail-fast
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps "${cargo_args[@]}"
cargo build --workspace --release "${cargo_args[@]}"
install_root="$(mktemp -d /tmp/jsonc-merge-install.XXXXXX)"
export PATH="$install_root/bin:$PATH"
cargo install --path crates/cli --root "$install_root" "${cargo_args[@]}"
jsonc-merge --version
node scripts/notices.cjs --check
node scripts/cross-parse.cjs "$install_root/bin/jsonc-merge"
node scripts/measure.cjs "$CARGO_TARGET_DIR/release/jsonc-merge" > "$evidence_dir/measurements.json"
source_hashes > "$evidence_dir/source-after.sha256"
diff -u "$evidence_dir/source-before.sha256" "$evidence_dir/source-after.sha256"
printf 'Offline verification passed. Evidence: %s\n' "$evidence_dir"
