#!/bin/bash
set -euo pipefail
runtime_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="$runtime_dir/.cache/cargo-home"
export CARGO_TARGET_DIR="$runtime_dir/.cache/target"
cargo +1.83.0 test --locked --offline --manifest-path "$runtime_dir/Cargo.toml"
cargo +1.83.0 fmt --manifest-path "$runtime_dir/Cargo.toml" -- --check
cargo +1.83.0 clippy --locked --offline --all-targets --manifest-path "$runtime_dir/Cargo.toml" -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.83.0 doc --locked --offline --no-deps --manifest-path "$runtime_dir/Cargo.toml"
