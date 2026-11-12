# Wiiform

Wiiform builds a new Wii NAND from a decrypted console seed and a declarative
recipe. The C++ core owns NAND and WiiQt operations. All three Rust commands
call that core in-process through direct CXX bindings. Qt types stay inside the native
implementation. The public C++ headers use standard-library types and
project-owned value types.

The command suite contains `wiiform` for recipe builds, `wii-nand` for focused
NAND operations and inspection, and `wii-seed` for seed export and unpacking.
The library opens source backups and inspected NANDs read-only.

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
mise run run:wii-nand -- stock-titles EUR
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
  --output prepared-console [--cache ./cache] [--offline] \
  [--reference same-console.nand.bin] \
  [--boot-policy stock|preserve-source] [--menu 4.3E|4.3U|4.3J]
```

The output must be a new directory. A successful build publishes `nand.bin`
and the matching BootMii `keys.bin` together. Builds that set up or import
display settings require `--reference` from the same console. The command
retains private evidence next to the bundle. Keep that directory protected:
the seed and reports contain console identity material. `--offline` forbids
downloads and requires the requested system titles and Priiloader payloads to
be in the cache.

The seed is plaintext console key material. Store it and any generated key
files securely. `wii-seed export BACKUP SEED` and `wii-seed unpack SEED DIR`
write plaintext outputs and refuse to overwrite existing targets.

## Direct commands

Run `wii-nand --help` and `wii-nand <command> --help` for the complete native
operation interface. Common commands include:

```sh
wii-nand stock-titles EUR
wii-nand create [--boot-policy stock] SEED NEW_NAND
wii-nand install-title --in-place [--offline] NAND CACHE TITLE_ID VERSION
wii-nand remove-title --in-place NAND TITLE_ID
wii-nand inspect-wad --sha256 SHA256 PACKAGE.wad
wii-nand inventory NAND NEW_REPORT.json
wii-nand preflight --candidate-only SEED CANDIDATE REPORT.json
```

Commands that mutate an existing image require `--in-place`. The native core
validates packages and available NAND space before mutation. Rich inspection
and inventory reports are JSON.

## Release packaging

Run `mise run package` on macOS arm64 or Linux x86_64. The task builds the
commands and writes an archive and its SHA-256 file to `dist/`. Set
`WIIFORM_DIST_DIR` to choose a different output directory.

The archive includes the three commands under `bin/`, runtime libraries under
`lib/`, and public C++ headers under `include/`. Keep the extracted directory
layout together. See the [migration guide](docs/migration.md) for the old recipe
mapping and the mise configuration for GitHub releases.

## License and attribution

Wiiform and migrated code are distributed under GPL-2.0-only. See
[`LICENSE`](LICENSE) and [`THIRD_PARTY.md`](THIRD_PARTY.md) for
the license and upstream attribution record.
