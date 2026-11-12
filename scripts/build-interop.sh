#!/usr/bin/env bash
set -euo pipefail

project=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project"

xmake build wiiform-core-write
xmake build wiiform-readonly

export WIIFORM_NATIVE_LIB_DIR="$project/build/lib"
cargo build --locked --release --target-dir "$project/build/cargo"
mkdir -p build/bin
install -m 755 build/cargo/release/wiiform-interop-smoke build/bin/
