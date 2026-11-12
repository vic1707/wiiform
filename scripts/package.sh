#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Assemble a versioned user archive using the Wiiform repository's native
# dependency packagers.
set -euo pipefail

usage() {
  echo "usage: $0 {macos-arm64|linux-x86_64} WIIFORM_REPO OUTPUT_DIR" >&2
  exit 2
}

[[ $# -eq 3 ]] || usage
platform=$1
repo=$(cd -- "$2" && pwd -P)
output_arg=$3
case "$platform" in
  macos-arm64)
    [[ $(uname -s)-$(uname -m) == Darwin-arm64 ]] || { echo "macos-arm64 must run on native Apple Silicon" >&2; exit 2; }
    ;;
  linux-x86_64)
    [[ $(uname -s)-$(uname -m) == Linux-x86_64 ]] || { echo "linux-x86_64 must run on native Linux x86_64" >&2; exit 2; }
    ;;
  *) usage ;;
esac

output_parent=$(cd -- "$(dirname -- "$output_arg")" && pwd -P)
output="$output_parent/$(basename -- "$output_arg")"
[[ -d $output && ! -L $output ]] || { echo "OUTPUT_DIR must be an existing real directory" >&2; exit 2; }

version=$(awk '/^version = / {gsub(/"/, "", $3); print $3; exit}' "$repo/Cargo.toml")
[[ $version =~ ^[0-9A-Za-z.+-]+$ ]] || { echo "could not read package version from Cargo.toml" >&2; exit 1; }
bundle="wiiform-$version-$platform"
stage="$output/$bundle"
archive="$output/$bundle.tar.gz"
checksum="$archive.sha256"
[[ ! -e $stage && ! -L $stage && ! -e $archive && ! -L $archive && ! -e $checksum && ! -L $checksum ]] || {
  echo "versioned stage or archive already exists; refusing to replace it" >&2
  exit 1
}

bindir="$repo/build/bin"
libdir="$repo/build/lib"
libexecdir="$repo/build/libexec"
for name in wiiform wii-seed wii-nand; do
  [[ -f $bindir/$name && -x $bindir/$name ]] || { echo "missing executable: $bindir/$name" >&2; exit 1; }
done
[[ -f $libexecdir/wiiform-readonly && -x $libexecdir/wiiform-readonly ]] || {
  echo "missing read-only worker: $libexecdir/wiiform-readonly" >&2; exit 1;
}
case "$platform" in
  macos-arm64) core="$libdir/libwiiform-core-write.dylib" ;;
  linux-x86_64) core="$libdir/libwiiform-core-write.so" ;;
esac
[[ -f $core ]] || { echo "missing native core library: $core" >&2; exit 1; }
[[ -f $repo/LICENSE && -f $repo/THIRD_PARTY.md && -f $repo/README.md ]] || {
  echo "required license or project documentation is missing" >&2; exit 1;
}
[[ -f $repo/schema/wiiform.recipe.schema.json ]] || { echo "recipe schema is missing" >&2; exit 1; }

artifacts=(
  "$bindir/wiiform=bin/wiiform"
  "$bindir/wii-seed=bin/wii-seed"
  "$bindir/wii-nand=bin/wii-nand"
  "$core=lib/$(basename -- "$core")"
  "$libexecdir/wiiform-readonly=libexec/wiiform-readonly"
)
case "$platform" in
  macos-arm64)
    python3 "$repo/scripts/package-macos.py" --platform macos-arm64 --output "$stage" \
      "${artifacts[@]/#/--artifact=}"
    ;;
  linux-x86_64)
    "$repo/scripts/package-linux.sh" "$stage" "${artifacts[@]}"
    ;;
esac

# Include user-facing docs and the generated recipe schema. The optional docs
# tree carries the migration guide when it exists in this checkout.
cp -- "$repo/LICENSE" "$repo/THIRD_PARTY.md" "$repo/README.md" "$stage/"
cp -R -- "$repo/schema" "$stage/schema"
mkdir -p "$stage/include"
cp -R -- "$repo/native/include/wiiform" "$stage/include/"
mkdir -p "$stage/examples"
cp -- "$repo/examples/recipe.yaml" "$stage/examples/"
if [[ -d $repo/docs ]]; then cp -R -- "$repo/docs" "$stage/docs"; fi
cat > "$stage/README.txt" <<'EOF'
Wiiform runtime bundle.
See README.md for commands, setup and recipe documentation.
EOF

tar -C "$output" -czf "$archive" "$bundle"
(cd "$output" && shasum -a 256 "$bundle.tar.gz") > "$checksum"
echo "archive: $archive"
echo "checksum: $checksum"
