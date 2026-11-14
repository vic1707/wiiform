# Wiiform

Wiiform builds a new Wii NAND from a decrypted console seed and a declarative
recipe. The C++ core owns NAND and WiiQt operations. All three Rust commands
call that core in-process through direct CXX bindings. Qt types stay inside the native
implementation. The public C++ headers use standard-library types and
project-owned value types.

The command suite contains `wiiform` for recipe builds, `wii-nand` for editing
an existing image into a new file, and `wii-seed` for extracting a seed to
stdout. Rust owns the NAND buffer and publishes files after the native
operation succeeds.

## Build from a checkout

Install the pinned tools and build from the repository root.

```sh
mise install
mise run build
mise run test
```

The build task configures Xmake, builds the C++ library, then builds and stages
the Rust commands in `build/bin/`. Mise gives Xmake a checkout-local temp
directory for its bundled Lua files. Use `mise tasks` to list individual tasks.

```sh
mise run run:wiiform -- --help
mise run run:wii-nand -- --help
mise run run:wii-seed -- --help
```

### Host dependencies

| Provided by | Requirements |
| --- | --- |
| mise | Xmake, CMake, Rust and Cargo, rustfmt, and Clippy |
| Xmake and Cargo | Pinned WiiQt, yaml-cpp, liblzma from xz, and locked Rust crates |
| Host | Git, a C++20 compiler and linker, make, and a Qt SDK with qmake, Qt Core, Gui, Widgets, and Network |
| Archive creation | Python 3 and Xcode linker tools on macOS. patchelf and pax-utils on Linux |
| Optional Priiloader source build | devkitPPC, libogc, and the pinned Priiloader submodule |

On macOS, install Xcode Command Line Tools and a Qt SDK. The build task
detects Homebrew Qt 5 under `/opt/homebrew/opt/qt@5`. Set `QT_ROOT_DIR` for a
different SDK. On Ubuntu 24.04, install `build-essential`, `git`,
`qtbase5-dev`, and `qt5-qmake`, then run `QT_ROOT_DIR=/usr mise run build`.
A first build retrieves dependencies if the caches are empty. The prebuilt
archive carries its Qt runtime.

## Recipe build

See the [example recipe](docs/recipe.example.yaml) and its
[editor schema](docs/wiiform.recipe.schema.json). Run `mise run schema` to
regenerate the schema from the Rust recipe model. Local file paths resolve
relative to the recipe. Every WAD entry has a required SHA-256. The recipe supports a 4.3E,
4.3U, or 4.3J System Menu; allowlisted channel removal; initial setup and
display settings; WAD installs; and Priiloader settings and named hacks.

```sh
wiiform --seed console.seed.yaml --recipe recipe.yaml \
  --output prepared-console \
  [--boot-policy stock|preserve-source] [--menu 4.3E|4.3U|4.3J]
```

NUS downloads are authenticated and kept in memory by default; no cache files
are read or written. To reuse verified downloads, set `WIIFORM_NUS_CACHE` to a
directory. For repeatable offline builds (including CI), set both
`WIIFORM_NUS_CACHE` and `WIIFORM_NUS_OFFLINE=1`. An offline cache miss fails.
The cache stores only files that pass Wii title signature and content checks.

The output must be a new directory. A successful build publishes `nand.bin`
and its matching BootMii `keys.bin` together. Setup or display settings generate
a fresh SYSCONF and mark first-run setup complete. Pair Wii Remotes again after
restoring a prepared image. A stock recipe without either section uses the
console's first-run setup. The release archive includes the pinned Priiloader
assets. When running from a checkout, run `mise run assets:priiloader` once before
using a Priiloader recipe. A `priiloader` section requires `settings`. Use
`settings: {}` for the pinned defaults. Each application replaces the settings
and disables unspecified hacks.

The seed contains plaintext console key material. Save the output privately.
For a footerless backup, place its matching 1,024-byte `keys.bin` next to it
or pass `--keys`.

```sh
(umask 077; wii-seed BACKUP > console.seed.yaml)
```

## Direct commands

```sh
wii-nand install-title INPUT OUTPUT TITLE_ID VERSION
wii-nand install-wad INPUT OUTPUT PACKAGE.wad --sha256 SHA256
wii-nand remove-title INPUT OUTPUT TITLE_ID
wii-nand install-priiloader INPUT OUTPUT
```

`wii-nand install-title` uses the same `WIIFORM_NUS_CACHE` and
`WIIFORM_NUS_OFFLINE` environment variables as `wiiform`.

Each edit requires a new output path. Run `wii-nand <command> --help` for
settings, signature policy and Priiloader hack options. The Priiloader edit
applies its defaults for unspecified settings and disables unspecified hacks.
An edit uses about 528 MiB of memory for the working NAND plus the payload.

## Release packaging

Run `mise run package` on macOS arm64 or Linux x86_64. The task builds the
commands and writes an archive and its SHA-256 file to `dist/`. Set
`WIIFORM_DIST_DIR` to choose a different output directory. The package task
prepares the pinned Priiloader assets from the upstream release ZIP. Set
`WIIFORM_PRIILOADER_ARCHIVE` to a local copy to package without fetching it.

The archive includes the three commands under `bin/`, runtime libraries under
`lib/`, Priiloader assets under `share/wiiform/`, and public C++ headers under
`include/`. Keep the extracted directory layout together. See the
[migration guide](docs/migration.md) for the old recipe mapping and the mise
configuration for GitHub releases.

## License and attribution

Wiiform and migrated code are distributed under GPL-2.0-only. See
[`LICENSE`](LICENSE) and [`THIRD_PARTY.md`](THIRD_PARTY.md) for
the license and upstream attribution record.
