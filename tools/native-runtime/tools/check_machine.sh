#!/bin/bash
set -euo pipefail
runtime_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "$#" != 1 ]]; then
  echo 'Usage: /bin/bash tools/native-runtime/tools/check_machine.sh /tmp/einvoice-machine-check' >&2
  exit 2
fi
work_dir="$1"
python3.12 "$runtime_dir/tools/verify_machine_sources.py"
python3.12 "$runtime_dir/tools/cache_dependencies.py" --cache-dir "$work_dir/cache"
export CARGO_HOME="$work_dir/cache/cargo-home"
export CARGO_TARGET_DIR="$work_dir/target"
cargo +1.83.0 test --locked --offline --manifest-path "$runtime_dir/Cargo.toml"
cargo +1.83.0 fmt --manifest-path "$runtime_dir/Cargo.toml" -- --check
cargo +1.83.0 clippy --locked --offline --all-targets --manifest-path "$runtime_dir/Cargo.toml" -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.83.0 doc --locked --offline --no-deps --manifest-path "$runtime_dir/Cargo.toml"
