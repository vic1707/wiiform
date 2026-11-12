#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
set -euo pipefail

if [[ $(uname -s)-$(uname -m) != Linux-x86_64 || $# -lt 2 ]]; then
    echo "Usage on Linux x86_64: $0 NEW_DIR SOURCE=DEST [SOURCE=DEST ...]" >&2
    exit 2
fi
command -v lddtree >/dev/null
command -v patchelf >/dev/null
destination=$1
shift
mkdir -- "$destination"
mkdir "$destination/lib"
declare -A copied=()
sources=()
images=()

for spec in "$@"; do
    source=$(realpath -- "${spec%%=*}")
    relative=${spec#*=}
    if [[ ! -f $source || $relative == "$spec" || $relative == /* || /$relative/ == */../* ]]; then
        echo "Invalid artifact: $spec" >&2
        exit 2
    fi
    output="$destination/$relative"
    mkdir -p "$(dirname -- "$output")"
    install -m 755 "$source" "$output"
    copied["$(basename -- "$output")"]=$source
    sources+=("$source")
    images+=("$output")
done

# lddtree prints the full dependency closure. Keep glibc at the host baseline
# and bundle the other resolved libraries, including distro-installed Qt.
for source in "${sources[@]}"; do
    dependencies=$(lddtree -l "$source")
    while IFS= read -r dependency; do
        [[ -n $dependency ]] || continue
        real=$(realpath -- "$dependency")
        [[ $real != "$source" ]] || continue
        name=$(basename -- "$dependency")
        case "$name" in
            ld-linux-x86-64.so.2|libc.so.6|libm.so.6|libpthread.so.0|libdl.so.2|librt.so.1|libresolv.so.2)
                continue ;;
        esac
        if [[ -v copied[$name] ]]; then
            [[ ${copied[$name]} == "$real" ]] || { echo "Conflicting shared library $name" >&2; exit 1; }
            continue
        fi
        output="$destination/lib/$name"
        install -m 755 "$real" "$output"
        copied["$name"]=$real
        images+=("$output")
    done <<< "$dependencies"
done

for image in "${images[@]}"; do
    case "$image" in
        "$destination/lib/"*) rpath='$ORIGIN' ;;
        *) rpath='$ORIGIN/../lib' ;;
    esac
    patchelf --set-rpath "$rpath" "$image"
done
