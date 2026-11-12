# Migration and architecture notes

Wiiform is the successor project to the host-side tools in `wiicli-tools`.
The behavioral reference for the port is commit `ab202ef`. The old checkout
holds the historical GUI comparison harness and console validation records.

## Components

```text
wiiform recipe CLI / wii-nand ── CXX adapter ── C++ writable core
                  │                                   │
wii-seed ── shared Rust worker client                 WiiQt
                  │                                   │
                  └── private worker ── C++ read-only core
```

The public C++ headers are under `src-wiiformlib/include/wiiform`. Xmake builds
`wiiform-core-write` and `wiiform-core-readonly` independently of Cargo.
The CXX adapter and its `rust/cxx.h` dependency live under `rust/interop`;
they are compiled by Cargo for the Rust callers. C++ consumers select the
native target and include `wiiform/write.hpp` or `wiiform/read.hpp`.
An application already using Qt can keep its own application instance.
Initialize the native API from the application's main thread.

To build a C++ consumer against an extracted archive, set `WIIFORM_HOME` to
its root and link the writable shared library:

```sh
c++ -std=c++20 -I"$WIIFORM_HOME/include" example.cpp \
  -L"$WIIFORM_HOME/lib" -lwiiform-core-write \
  -Wl,-rpath,"$WIIFORM_HOME/lib" -o example
```

Build `wiiform-core-readonly` from source to link a read-only C++ consumer.

The Rust binaries own argument parsing, recipe validation, build sequencing,
and bundle publication. Their native bindings call named functions with
strings, byte vectors, flat enums and simple name/value overrides. NAND images
are passed by path so native file locking and staged-write behavior can remain
in the C++ layer. Qt classes and WiiQt ownership types stay private to C++.
Rich diagnostic reports cross the boundary as JSON strings and are decoded by
Rust.

The read-only worker is intentionally a separate process built against the
read-only WiiQt target. It handles source NAND authentication, inventories,
preflight checks and package inspection. The Rust workflow resolves the worker relative to the installed executable. NAND
mutations use the writable C++ library. This preserves the split between
inspection and write authority while sharing the same high-level build flow.

## Native implementation and provenance

The project-owned C++ library wraps the following areas:

- **WiiQt** provides the NAND filesystem, title access and related console
  formats. The xmake package pins the `vic1707/wiiqt-tools` fork at commit
  `42536a2c3becf384687a64a3d9ddadad435dc22b` (package version `0.1.1`).
- **Wiiform C++ code** owns the narrow library API, boot validation, seed
  reconstruction, title creation/install/removal, WAD validation and install,
  SYSCONF writes, Priiloader files/settings, and BackupMii footer/key-sidecar
  handling. The CXX adapter is glue; it converts Rust-friendly values into
  native calls and maps exceptions to Rust errors.
- **Priiloader** is included as an upstream git submodule for source access and
  the Wii-side installer reference, pinned to commit
  `2b70b3693e9dc3e144db1a73224921d405081663` (0.10.0). Wiiform's host-side
  NAND installation and settings support are project code. Runtime installer
  app and hack-definition assets are verified against pinned hashes before
  use. Building that app from the submodule is an optional development task
  and additionally requires devkitPPC, libogc, and their environment setup;
  normal host builds use the pinned release payload.
- **Rust dependencies** provide CLI parsing (`clap`), serialization and JSON
  (`serde`, `serde_json`), recipe YAML parsing (`yaml-rust2`), schema generation
  (`schemars`), hashing (`sha2`), temporary files (`tempfile`), C++ interop
  (`cxx`), and supporting crates. Exact versions are recorded in Cargo files.

The external native build dependencies declared to Xmake include pinned
`yaml-cpp` and `xz`; Xmake also builds the pinned WiiQt package. Qt is found
from an installed SDK and is not currently managed by mise. The build host
also supplies its C++ compiler, linker, `qmake`, and `make`. See the project
README for platform setup details. A release archive is intended to bundle
non-system Qt/runtime libraries, so end users should not need the development
SDK after release packaging is complete.

## Commands and supported operations

`wii-seed` exports the selected console inputs from a backup into a plaintext
seed and can unpack a seed for inspection. `wii-nand` exposes focused native
operations and read-only report commands: stock title tables, NAND creation,
NUS title installs, WAD inspection and install, allowlisted channel removal,
display/SYSCONF handling, Priiloader, preflight, and inventory. `wiiform`
combines those operations into one verified build from a seed and recipe.

Wiiform recipes are a small declarative DSL validated by a Rust model. The
model drives the generated editor schema. The workflow verifies each input
hash, checks WAD signature policy and package identity, prepares stock titles
in deterministic title-ID order, records inventory evidence after each
requested stage, and refuses to replace an existing output directory. It
publishes the NAND and its matching `keys.bin` as one directory operation.
Failures retain private evidence for diagnosis and do not publish the bundle.

Current recipe features are documented in `../README.md` and the example
recipe. Future recipe fields should remain declarative: the recipe specifies
the desired operation, while native library functions perform format-aware
validation and mutation.

## Build and host prerequisites

`mise install` installs the pinned project tools, including xmake, CMake and
Rust. `mise run build` runs the native xmake build followed by the locked
Cargo build. Xmake resolves the pinned native source packages, but Qt and the
platform's basic C++ toolchain are currently provisioned outside mise.

For macOS, install Xcode Command Line Tools and a Qt SDK with `qmake` and the
Qt Gui, Widgets, and Network components. For Linux x86_64, install a C++
compiler/linker, make, and a compatible Qt development SDK exposing the same
components. Then configure/build with mise. This is the current dependency
boundary; completely automated Qt provisioning may be considered separately.

## Update an existing recipe

Move keys under the old `system:` block up one indentation level. Keep
`menu`, `remove`, `setup`, `display`, `titles`, and `priiloader` as top-level
keys. WAD paths remain relative to the recipe and require SHA-256. For a WAD
that the recipe deliberately permits without a Nintendo signature, use
`signature_policy: allow-unsigned`. Existing decrypted seeds remain valid.
Private checkpoints stay in test tooling. The normal recipe command publishes
the final NAND and matching keys together.

## Release archive and mise

Run `scripts/package.sh PLATFORM REPO OUTPUT_DIR` after building on
`macos-arm64` or `linux-x86_64`. The archive includes the private worker
under `libexec` and the non-system library dependencies under `lib`.
Once the matching archives and checksums are published on GitHub, a mise
configuration can select the main command from the archive:

```toml
[tools]
"github:OWNER/wiiform" = { version = "0.1.0", strip_components = 1, bin_path = "bin" }
```

Replace `OWNER` with the release repository owner. The archived `wiiform`
command invokes its private worker directly; the two public utility commands
are available for standalone operations.
