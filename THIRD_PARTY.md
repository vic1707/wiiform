# Third-party components and attribution

Wiiform's project license is GPL-2.0-only. Migrated source retains original
copyright and SPDX headers. The final source and binary release must include
the applicable upstream license notices and corresponding license texts.

| Component | Use | Source / pin | License handling |
| --- | --- | --- | --- |
| WiiQt NAND tools | NAND filesystem, title and console-format operations | [vic1707/wiiqt-tools](https://github.com/vic1707/wiiqt-tools), pinned commit `42536a2c3becf384687a64a3d9ddadad435dc22b` with a [local read-only access patch](xmake/patches/wiiqt-read-only.patch) | GPL-2.0-only; preserve its `COPYING` and notices in source/release bundles |
| Priiloader | Wii-side installer project, source reference and submodule | [DacoTaco/priiloader](https://github.com/DacoTaco/priiloader), vendored submodule | Preserve the submodule's upstream license files/notices; Wiiform host integration remains separately identified as project code |
| Qt | Native runtime and GUI/Widgets/Network libraries used by WiiQt | Host Qt SDK during build; exact detected SDK recorded by xmake package configuration | Follow the license notices shipped with the Qt distribution and include applicable notices in binary archives |
| yaml-cpp | YAML support in the native dependency graph | Xmake package pinned to 0.8.0 | Include upstream license notice in source and binary attribution bundle |
| xz / liblzma | Compression support in the native dependency graph | Xmake package pinned to 5.8.4 | The linked [liblzma](https://tukaani.org/xz/liblzma-api/) component is 0BSD |
| Rust crates | CLI, parsing, schema, hashing, CXX bridge and supporting functions | Exact resolved graph in `Cargo.lock` | Preserve each crate's license metadata/notices; generate a complete notice list before publishing binaries |

## Build tools and packaging tools

Xmake, CMake, the Rust toolchain/Cargo, and platform compiler/build utilities
are development tools rather than Wiiform runtime features. They are pinned
where practical in `.mise.toml`; Qt and the platform C++ toolchain currently
need independent host provisioning. Release packaging must include all
required non-system runtime libraries and their notices, while relying only
on platform system libraries outside the archive.

## Distribution checklist

Before a public release:

- Include Wiiform's GPL-2.0-only `LICENSE` and preserve all source notices.
- Include WiiQt and Priiloader upstream license texts and attribution.
- Generate a complete license/notice inventory for the locked Rust graph,
  yaml-cpp, xz, and the shipped Qt runtime.
- Verify that the Linux and macOS archives contain every required runtime
  license notice and checksum file.
- Publish corresponding source and build/packaging scripts with each binary
  release, consistent with the applicable licenses.

This inventory is a migration-stage record, not a legal determination. The
final release packaging review must verify exact upstream license files and
Qt distribution obligations for the selected SDK/runtime.
