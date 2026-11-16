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

Rust owns CLI arguments, seed and recipe validation, the NAND allocation, and
publication. C++ borrows the buffer for each synchronous operation. The public
headers use `std::span` and project-owned value types. CXX translates native
exceptions to Rust errors. Callers can discard a working buffer if an error
occurs after writes begin.

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

- **WiiQt** provides the NAND filesystem, title installation, the NUS
  downloader and title lists, and the setting.txt cipher. The xmake package
  pins a commit of the `vic1707/wiiqt-tools` fork, which carries read-only file
  access and a random-access buffer backend. Set `WIIQT_GIT_URL` to a local
  checkout's `.git` directory to build against unpublished fork commits.
- **Wiiform C++ code** exposes one function per operation in
  `wiiform-lib/include/wiiform/` (no Qt, CXX or Rust types): seed extraction,
  image initialization, single-title install, WAD and cIOS install, channel
  removal, SYSCONF creation, the three Priiloader steps, the BackupMii header
  and the verifications. It owns boot validation, WAD policy, capacity
  planning and the checks around WiiQt. Rust sequences these calls: the stock
  build is a loop over `stock_titles`. The CXX adapter only converts values;
  the bridge binds the library's own enums and maps exceptions to Rust errors.
- **Priiloader** 0.10.0 (upstream commit
  `2b70b3693e9dc3e144db1a73224921d405081663`) is the reference for the
  Wii-side installer. Wiiform's host-side
  NAND installation and settings support are project code. Release packaging
  extracts the installer app, hack definitions, and license from the pinned
  upstream ZIP. The native writer verifies the payload hashes.
- **Rust dependencies** provide CLI parsing (`clap`), serialization and JSON
  (`serde`, `serde_json`), YAML parsing (`serde-saphyr`), schema generation
  (`schemars`), hashing (`sha2`), temporary files (`tempfile`), C++ interop
  (`cxx`), and supporting crates. Exact versions are recorded in Cargo files.

Xmake builds the pinned WiiQt package. The host supplies its C++ toolchain,
`make`, and Qt SDK with `qmake`. Release archives carry the Qt runtime.
The [README](../README.md#host-dependencies) lists the prerequisites.

## Commands and supported operations

Use `wii-seed BACKUP` to extract from a backup with an embedded footer. For a
footerless backup, add `--keys FILE` with its matching 1,024-byte sidecar.
Both commands write plaintext seed YAML to stdout. Use
`wiiform` for a new image and `wii-nand` for title installs, channel removal,
and combined Priiloader installation and configuration on an existing image.
Each editor command takes separate input and output paths.

The Rust recipe model drives the generated editor schema. Wiiform validates
the declared recipe before applying a menu override and validates it again
after retargeting. It publishes the NAND and its matching `keys.bin` as one
directory operation after the last native check succeeds.

## Developer tasks

`mise run build` configures and builds Xmake before running Cargo.
`mise run test` runs the C++, Rust, Clippy, and schema checks. Use
`mise run schema` to regenerate the JSON Schema without a native build.
CI generates the schema and checks it against the committed copy under
`docs/`.

## Update an existing recipe

Move keys under the old `system:` block up one indentation level. Keep
`menu`, `remove`, `setup`, `titles`, `cios`, `hbc`, and `priiloader` as top-level keys.
Move `display` under `setup`. Move each WAD's `path` or `inline` under `source`. Keep `sha256`
beside `source`. WAD paths resolve relative to the recipe.
For inline data, use `inline: !!binary |` followed by indented base64. Either source
requires a SHA-256 of the raw WAD bytes. For a WAD that the recipe deliberately
permits without a Nintendo signature, replace `signature_policy: allow-unsigned`
with `allow_unsigned: true`. List channels to `remove` by name; title IDs are
no longer accepted there.
Regenerate earlier unreleased seeds with the current `wii-seed`.
Remove `--reference` from scripts. A `setup` mapping, including any
`setup.display` options, generates SYSCONF and completes first-run setup.
Omit `setup` or set it to null to keep the wizard enabled. Use a two-letter country code
such as GB, US, or JP to set a country. Quote "NO" for Norway because some YAML
parsers interpret it as a boolean. Replace quoted Wii codes such as "110"
with the corresponding ISO code. Pair Wii Remotes after restore.

Use `WIIFORM_NUS_CACHE` for a persistent title cache and `WIIFORM_NUS_OFFLINE=1`
to require cache hits. Online builds create the configured cache. Title
downloads stay in memory by default.
Use `priiloader: {}` to install with pinned defaults. Remove old `app` fields
and the `--app` and
`--hack-definitions` flags. The commands use the bundled installer and definitions.
Move the keys under `priiloader.settings` up into `priiloader`, replace
`black_background: false` with `white_background: true`, and turn the `hacks`
mapping into a list of the hacks to enable, such as `hacks: [fix_480p]`.
On the command line, pass `--stop-disc` or `--white-background` to enable a
setting and `--hack fix_480p` to enable a hack.
Rebuild from the seed and recipe to migrate an early image containing the
480p-only Priiloader definitions.

## What a seed contributes

The seed stores the exact 1,024-byte key footer and the XZ-compressed boot area
as YAML binary fields. It also stores bad-block numbers and the eight readable
`setting.txt` fields. SHA-256 covers the footer, the decompressed boot area, and
the encoded original settings. Wiiform verifies these before retargeting the
settings to the requested menu. Optional `info` shows the recognized BootMii
status and readable footer header.

The seed digest covers the original footer, including its text header and
padding. Stock creation writes a BackupMii text header using the OTP console
ID and emits a warning if the header changed. The binary key data and its
padding remain byte-identical to the seed. The build publishes the verified
NAND footer as `keys.bin`.
Use `wiiform --seed -` to read plaintext YAML from stdin.

## Release archive and mise

Run `mise run package` on macOS arm64 or Linux x86_64 to build the archive.
Once the archives and checksums are published on GitHub, this mise
configuration installs the commands with their shared native runtime.

```toml
[tools]
"github:vic1707/wiiform" = { version = "0.1.0", strip_components = 1, bin_path = "bin" }
```
