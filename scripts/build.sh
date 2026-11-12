#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
set -euo pipefail

project=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project"
xmake build -y -j2 wiiform-core-write
xmake build -y -j2 wiiform-readonly
export WIIFORM_NATIVE_LIB_DIR="$project/build/lib"
cargo build --locked --release --bins --target-dir "$project/build/cargo"
mkdir -p build/bin
for command in wii-seed wii-nand wiiform; do
    if [[ -f "build/cargo/release/$command" ]]; then
        install -m 755 "build/cargo/release/$command" build/bin/
    fi
done
