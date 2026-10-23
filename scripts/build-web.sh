#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="$repo_dir/web/dist"

cargo build --manifest-path "$repo_dir/Cargo.toml" --locked --release \
  --target wasm32-unknown-unknown --features editor --lib

mkdir -p "$dist_dir/pkg"
wasm-bindgen "$repo_dir/target/wasm32-unknown-unknown/release/opencity.wasm" \
  --out-dir "$dist_dir/pkg" --target web --no-typescript
cp "$repo_dir/web/index.html" "$dist_dir/index.html"
cp "$repo_dir/web/favicon.svg" "$dist_dir/favicon.svg"
touch "$dist_dir/.nojekyll"

printf 'Web build ready: %s\n' "$dist_dir"
