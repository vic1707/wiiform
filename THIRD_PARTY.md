# Third-party components and attribution

Wiiform's project license is GPL-2.0-only. Migrated source retains original
copyright and SPDX headers. The final source and binary release must include
the applicable upstream license notices and corresponding license texts.

| Component | Use | Source / pin | License handling |
| --- | --- | --- | --- |
| WiiQt NAND tools | NAND filesystem, title and console-format operations | [vic1707/wiiqt-tools](https://github.com/vic1707/wiiqt-tools), commit pinned in [xmake/wiiqt-nand.lua](xmake/wiiqt-nand.lua) | GPL-2.0-only; preserve its `COPYING` and notices in source/release bundles |
| Priiloader | Wii-side installer app and hack definitions, and the source reference for the host-side installation | [DacoTaco/priiloader](https://github.com/DacoTaco/priiloader) 0.10.0, commit `2b70b3693e9dc3e144db1a73224921d405081663`; `priiloader.app` (SHA-256 `f4508801cc4274bbcb74c07551d591486209c634fbf44f65765685fb19ec0157`) and `hacks_hash.ini` (SHA-256 `aea1a0009a327298f0608809c240aaf0ca5f5de3b12ef7ad761901dc4cb74ff4`) are redistributed unmodified under `wiiform-cli/assets/priiloader/` and compiled into the commands | GPL-2.0; its `LICENSE` is committed beside the files; Wiiform host integration remains separately identified as project code |
| RVLoader | Bootloader stored as Priiloader's boot file and Wi-Fi driver module for the derived IOS, when a recipe asks for `rvloader`; reference for how the installer derives its IOS | [Aurelio92/RVLoader](https://github.com/Aurelio92/RVLoader), `installer/data/bootloader.dol` and `installer/data/wd.bin` at the release tag the recipe names, checked against the recipe's SHA-256 values. `wiiform-lib/src/nand/RvloaderIos.cpp` reproduces the installer's module layout and byte patches | GPL-3.0; the two files are downloaded at build time into the user's NAND image, not redistributed in Wiiform's source or binaries |
| Qt | Native runtime: the Core and Network libraries used by WiiQt | Host Qt SDK during build; exact detected SDK recorded by xmake package configuration | Follow the license notices shipped with the Qt distribution and include applicable notices with binary releases |
| Rust crates | CLI, parsing, schema, hashing, XZ compression, CXX bridge and reqwest HTTPS with Rustls/AWS-LC | Exact resolved graph in `Cargo.lock` | Preserve each crate's license metadata/notices; generate a complete notice list before publishing binaries |

The `aws-lc-sys` crate declares Apache-2.0 among its required licenses. Its
use in the GPL-2.0-only binary needs review before public distribution.
[Apache Software Foundation compatibility notes](https://www.apache.org/licenses/GPL-compatibility)

## Build tools and packaging tools

Xmake, CMake, the Rust toolchain/Cargo, and platform compiler/build utilities
are development tools rather than Wiiform runtime features. They are pinned
where practical in `.mise.toml`; Qt and the platform C++ toolchain currently
need independent host provisioning. The container images take the Qt runtime
from Debian packages, which carry their own notices.

## Distribution checklist

Before a public release:

- Include Wiiform's GPL-2.0-only `LICENSE` and preserve all source notices.
- Include WiiQt and Priiloader upstream license texts and attribution.
- Generate a complete license/notice inventory for the locked Rust graph,
  and the shipped Qt runtime.
- Verify that the container images contain every required runtime license
  notice.
- Publish corresponding source and build/packaging scripts with each binary
  release, consistent with the applicable licenses.

This inventory is a migration-stage record, not a legal determination. The
final release packaging review must verify exact upstream license files and
Qt distribution obligations for the selected SDK/runtime.
