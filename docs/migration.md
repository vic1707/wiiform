# Migration and architecture notes

Wiiform is the successor project to the host-side tools in `wiicli-tools`.
The behavioral reference for the port is commit `ab202ef`. The old checkout
holds the historical GUI comparison harness and console validation records.

## Components

```text
wiiform / wii-nand / wii-seed
        │  named CXX calls
        ▼
wiiform-core  →  WiiQt and Qt
```

Xmake builds `wiiform-core` independently of Cargo. The C++ public headers
are under `wiiform-lib/include/wiiform` and the implementation is under
`wiiform-lib/src`. Cargo builds the commands under `wiiform-cli`. The
CXX adapter lives in `wiiform-cli/src/interop`.

Rust owns CLI arguments, recipe orchestration, and bundle publication.
Native calls handle NAND paths, title IDs, and settings values directly.
C++ returns JSON for rich reports and typed values for stock titles and
authenticated SYSCONF data. Seed export, inventory, preflight, and source
settings import open input NANDs with WiiQt's explicit read-only mode.

To use the public C++ API, include `wiiform/read.hpp` or `wiiform/write.hpp`.
Initialize the native API from the application's main thread. An application
that already uses Qt can keep its own application instance. To link against
the extracted archive, set `WIIFORM_HOME` to its root.

```sh
c++ -std=c++20 -I"$WIIFORM_HOME/include" example.cpp \
  -L"$WIIFORM_HOME/lib" -lwiiform-core \
  -Wl,-rpath,"$WIIFORM_HOME/lib" -o example
```

## Native implementation and provenance

The project-owned C++ library wraps the following areas:

- **WiiQt** provides the NAND filesystem, title access and related console
  formats. The xmake package pins the `vic1707/wiiqt-tools` fork at commit
  `42536a2c3becf384687a64a3d9ddadad435dc22b` with a local patch for per-image read-only access.
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
`yaml-cpp` and `xz`. Xmake also builds the pinned WiiQt package. The host
supplies its C++ toolchain, `make`, and Qt SDK with `qmake`. Release archives
carry the Qt runtime. The [README](../README.md#host-dependencies) lists the
prerequisites.

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

## Developer tasks

`mise run build` configures and builds Xmake before running Cargo.
`mise run test` runs the C++, Rust, Clippy, and schema checks. Use
`mise run schema` to regenerate the JSON Schema without a native build.
CI generates the schema and checks it against the committed copy under
`docs/`.

## Update an existing recipe

Move keys under the old `system:` block up one indentation level. Keep
`menu`, `remove`, `setup`, `display`, `titles`, and `priiloader` as top-level
keys. WAD paths remain relative to the recipe and require SHA-256. For a WAD
that the recipe deliberately permits without a Nintendo signature, use
`signature_policy: allow-unsigned`. Existing decrypted seeds remain valid.
Private checkpoints stay in test tooling. The normal recipe command publishes
the final NAND and matching keys together.

## Release archive and mise

Run `mise run package` on macOS arm64 or Linux x86_64 to build the archive.
Once the archives and checksums are published on GitHub, this mise
configuration installs the commands with their shared native runtime.

```toml
[tools]
"github:vic1707/wiiform" = { version = "0.1.0", strip_components = 1, bin_path = "bin" }
```
