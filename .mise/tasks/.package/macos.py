#!/usr/bin/env python3
"""Stage Wiiform and its dynamic libraries as a relocatable macOS bundle."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys


def run(*args: str) -> str:
    p = subprocess.run(args, check=True, text=True, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE)
    return p.stdout


def macho_deps(path: Path) -> list[str]:
    lines = run("otool", "-L", str(path)).splitlines()[1:]
    names = [line.strip().split(" (compatibility", 1)[0] for line in lines]
    # otool -L lists a dylib's own install name ahead of its dependencies.
    ids = run("otool", "-D", str(path)).splitlines()[1:]
    return names[1:] if names and ids and names[0] == ids[0].strip() else names


def macho_rpaths(path: Path) -> list[str]:
    lines = run("otool", "-l", str(path)).splitlines()
    found = []
    for i, line in enumerate(lines):
        if line.strip() == "cmd LC_RPATH":
            for row in lines[i + 1:i + 5]:
                match = re.search(r"path (.+?) \(offset", row)
                if match:
                    found.append(match.group(1))
                    break
    return found


def is_system(name: str) -> bool:
    return name.startswith(("/System/Library/", "/usr/lib/"))


def framework_root(path: Path) -> Path | None:
    parts = path.parts
    for i, part in enumerate(parts):
        if part.endswith(".framework"):
            return Path(*parts[:i + 1])
    return None


def resolve_macho(name: str, loader: Path, rpaths: list[str]) -> Path | None:
    candidates = []
    if name.startswith("@loader_path/"):
        candidates.append(loader.parent / name[len("@loader_path/"):])
    elif name.startswith("@executable_path/"):
        candidates.append(loader.parent / name[len("@executable_path/"):])
    elif name.startswith("@rpath/"):
        suffix = name[len("@rpath/"):]
        for rpath in rpaths:
            base = rpath.replace("@loader_path", str(loader.parent)).replace("@executable_path", str(loader.parent))
            candidates.append(Path(base) / suffix)
    elif name.startswith("/"):
        candidates.append(Path(name))
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()
    return None


def mac_bundle(args: argparse.Namespace) -> None:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("macOS arm64 packaging must run on a native arm64 Mac")
    out = args.output.resolve()
    if out.exists() and any(out.iterdir()):
        raise RuntimeError(f"output directory is not empty: {out}")
    (out / "bin").mkdir(parents=True, exist_ok=True)
    (out / "lib").mkdir(exist_ok=True)

    # Keep a source path -> packaged load path map. Frameworks preserve their
    # normal versioned directory and symlink layout; dylibs use lib/deps.
    copied: dict[Path, Path] = {}
    images: set[Path] = set()
    queue: list[Path] = []
    artifact_rows: list[tuple[Path, Path]] = []
    for spec in args.artifact:
        src_text, sep, dest_text = spec.partition("=")
        if not sep:
            raise RuntimeError(f"artifact must be SOURCE=relative/path: {spec}")
        src = Path(src_text).resolve(strict=True)
        dest = Path(dest_text)
        if dest.is_absolute() or ".." in dest.parts:
            raise RuntimeError(f"artifact destination must stay in bundle: {dest}")
        target = out / dest
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, target)
        artifact_rows.append((src, target))
        images.add(src)
        queue.append(src)
        copied[src] = target

    def add_dependency(source: Path) -> None:
        root = framework_root(source)
        if root:
            dest_root = out / "lib" / root.name
            if not dest_root.exists():
                shutil.copytree(root.resolve(), dest_root, symlinks=True)
            actual = source.resolve()
            target = dest_root / actual.relative_to(root.resolve())
            # Framework current-version links can resolve differently; map the
            # resolved source and the load-name spelling separately below.
            copied[actual] = target
            images.add(actual)
            queue.append(actual)
        else:
            actual = source.resolve()
            target = out / "lib" / "deps" / actual.name
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.exists():
                shutil.copy2(actual, target)
            copied[actual] = target
            images.add(actual)
            queue.append(actual)

    # Discover the complete non-system dependency closure. Build artifacts may
    # contain build-tree absolute rpaths; those are used only to locate inputs,
    # then removed from the packaged images.
    while queue:
        source = queue.pop()
        for name in macho_deps(source):
            if is_system(name):
                continue
            dependency = resolve_macho(name, source, macho_rpaths(source))
            if dependency is None:
                raise RuntimeError(f"cannot resolve dependency {name!r} from {source}")
            if dependency not in copied:
                add_dependency(dependency)

    # Rewrite dependency references in every copied image and make each one
    # search the bundle's lib directory relative to itself.
    for source in sorted(images, key=str):
        target = copied[source]
        for name in macho_deps(source):
            if is_system(name):
                continue
            dependency = resolve_macho(name, source, macho_rpaths(source))
            if dependency is None or dependency not in copied:
                raise RuntimeError(f"missing copied dependency {name!r} in {source}")
            dep_target = copied[dependency]
            rel = dep_target.relative_to(out / "lib").as_posix()
            run("install_name_tool", "-change", name, "@rpath/" + rel, str(target))
        if framework_root(source) or source.suffix == ".dylib":
            rel = target.relative_to(out / "lib").as_posix()
            run("install_name_tool", "-id", "@rpath/" + rel, str(target))
        rel_lib = os.path.relpath(out / "lib", target.parent)
        rpath = "@loader_path" if rel_lib == "." else "@loader_path/" + rel_lib
        for old in macho_rpaths(source):
            subprocess.run(["install_name_tool", "-delete_rpath", old, str(target)],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        run("install_name_tool", "-add_rpath", rpath, str(target))

    # Ad-hoc signatures are sufficient for local relocation smoke tests. Sign
    # nested code first; production notarization belongs to the release job.
    for target in sorted({copied[src] for src in images}, key=lambda p: -len(p.parts)):
        run("codesign", "--force", "--sign", "-", str(target))
    for _, target in artifact_rows:
        run("codesign", "--force", "--sign", "-", str(target))

    # Verify no packaged Mach-O retains a user, Homebrew, or build-tree path.
    for target in sorted({copied[src] for src in images} | {t for _, t in artifact_rows}):
        for name in macho_deps(target):
            if name.startswith("/") and not is_system(name):
                raise RuntimeError(f"non-relocatable load path remains: {target}: {name}")
        for rpath in macho_rpaths(target):
            if rpath.startswith("/"):
                raise RuntimeError(f"non-relocatable rpath remains: {target}: {rpath}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--artifact", action="append", default=[],
                        help="native input as SOURCE=relative bundle path, repeatable")
    args = parser.parse_args()
    try:
        mac_bundle(args)
    except (OSError, subprocess.CalledProcessError, RuntimeError) as exc:
        print(f"package: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
