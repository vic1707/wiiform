#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
set -euo pipefail

project=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project"
export WIIFORM_NATIVE_LIB_DIR="$project/build/lib"
case $(uname -s) in
  Darwin) export DYLD_LIBRARY_PATH="$WIIFORM_NATIVE_LIB_DIR${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" ;;
  Linux) export LD_LIBRARY_PATH="$WIIFORM_NATIVE_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ;;
  *) echo "Unsupported host" >&2; exit 1 ;;
esac

cargo fmt --all --check
cargo clippy --locked --release --workspace --all-targets --target-dir build/cargo
cargo test --locked --release --workspace --target-dir build/cargo
cargo run --quiet --locked --release --example recipe_schema --target-dir build/cargo |
  cmp schema/wiiform.recipe.schema.json -

for suite in install inventory preflight remove roundtrip sysconf wad backupmii_footer public-api; do
  xmake build -y -j2 "${suite}-tests"
  case "$suite" in
    inventory) build/bin/inventory-tests build/bin/wii-nand ;;
    roundtrip) build/bin/roundtrip-tests build/bin/wii-seed ;;
    *) "build/bin/${suite}-tests" ;;
  esac
done
