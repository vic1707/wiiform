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
| Xmake and Cargo | Pinned WiiQt and locked Rust crates |
| Host | Git, a C++20 compiler and linker, make, and a Qt SDK with qmake, Qt Core, Gui, Widgets, and Network |
| Archive creation | Python 3 and Xcode linker tools on macOS. patchelf and pax-utils on Linux |

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
relative to the recipe. A WAD entry has exactly one `source` key. Use `path`
for a local file, `inline: !!binary |` for a base64 block, or `url` for an
HTTPS download. Each source requires the SHA-256 of the raw WAD bytes.
The recipe supports 4.3E, 4.3U, and 4.3J, along with allowlisted
channel removal, initial setup, display settings, and Priiloader settings and named hacks. Set setup country
codes using uppercase ISO alpha-2 values such as `GB`, `US`, and `JP`.

URL sources work for both `titles` and `cios`. Downloads use system TLS
trust, follow up to five HTTPS redirects, and have a 60-second total
timeout and the same size limit as a local WAD. Use a local `path` or
`inline` for a network-free build. `WIIFORM_NUS_OFFLINE` controls only
Nintendo title downloads.

WiiQt's own diagnostic messages are hidden; set `WIIFORM_QT_MESSAGES=1` to see
them when investigating a failure.

Use the `titles` list for downloadable channels and Nintendo-signed system
channels or Wii IOS WADs for slots 3–253. `allow_unsigned: true` applies to
downloadable channels. Each entry verifies an exact installed match or installs
the WAD when the title is absent. An exact match emits a warning. A conflicting
title or incomplete installation stops the build. Stock 4.3 includes IOS9 v1034
in slot 9. List that WAD under `titles` with the default Nintendo signature
policy to verify the dependency in an Ocarina of Time recipe.
`wii-nand install-wad` writes a byte-identical copy to the requested output when
it verifies a match.

```sh
wiiform --seed console.seed.yaml --recipe recipe.yaml \
  --output prepared-console \
  [--boot-policy stock|preserve-source] [--menu 4.3E|4.3U|4.3J]
```

NUS downloads are authenticated and kept in memory by default; no cache files
are read or written. To reuse verified downloads, set `WIIFORM_NUS_CACHE` to a
directory. Online builds create it automatically. For repeatable offline
builds (including CI), set both
`WIIFORM_NUS_CACHE` and `WIIFORM_NUS_OFFLINE=1`. An offline cache miss fails.
The cache stores only files that pass Wii title signature and content checks.

The output must be a new directory. A successful build publishes `nand.bin`
and its matching BootMii `keys.bin` together. A `setup` mapping completes
first-run setup using your values and defaults for omitted fields. Put display
options such as the aspect ratio under `setup.display`. Use `setup: {}` to
apply all defaults. Omit `setup` or set it to `null` to run the Wii setup wizard.
Pair Wii Remotes
again after restoring a prepared image.

Wiiform uses the pinned Priiloader assets included in the release archive.
When running from a checkout, run `mise run assets:priiloader` once before
using a Priiloader recipe. Use `priiloader: {}` to install with pinned defaults.
Settings sit directly under `priiloader`, and `hacks` lists the hacks to
enable. Each application replaces the settings and disables unlisted hacks.

### Homebrew Channel

```yaml
hbc:
  reborn:
    version: "1.16.0"
```

Give `version` as a semantic version. Wiiform looks up the corresponding
GitHub release and downloads its retail OHBC WAD. It verifies the GitHub
asset’s SHA-256 by default. Add `sha256` under `reborn` to pin the exact WAD
in your recipe.
`WIIFORM_NUS_OFFLINE` governs Nintendo downloads only. [HBC Reborn releases](https://github.com/Monsterray/hbc-reborn/releases)

```yaml
hbc:
  custom:
    source:
      path: wads/my-homebrew-channel.wad
    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
```

If you used the earlier `hbc.variant` form, move its fields under `hbc.reborn`
or `hbc.custom`.

A custom WAD requires its SHA-256. The source may be a recipe-relative path,
inline binary, or an HTTPS URL. Wiiform validates the package and checks its
title ID against the Homebrew Channel IDs recognized by Priiloader before
writing it. The title installs before Priiloader settings are applied, so
`autoboot: homebrew_channel` can use it.

The original HackMii HBC 1.1.2 is available through its Wii-side installer.
A built-in recipe variant would require a verified offline package from that
release. [Official HackMii Installer](https://bootmii.org/download/)

### cIOS packages

Use `cios` entries for prebuilt, hash-pinned Wii d2x WADs. Each entry gives
the expected slot, base IOS, and TMD revision. Wiiform checks the Wii TMD,
ticket, decrypted content hashes, and the d2x IOSINFO record. The SHA-256
pins the package you obtained from your chosen source.

Slots 248–251 and bases 38, 56, 57, and 58 are supported. An empty slot can be
installed, and an exact installed match is verified and reported. Set
`replace_stock_stub: true` to replace the verified Nintendo v65280 stub in
slot 249 or 250. Other occupied slots only accept an exact match.

```sh
wii-nand install-cios INPUT_NAND OUTPUT_NAND D2X_WAD --sha256 HEX \
  --slot 249 --base 56 --revision 65535 --replace-stock-stub
```

The command also accepts an HTTPS URL in place of the WAD path and uses the
same slot and package checks as the recipe. See the
[example recipe](docs/recipe.example.yaml) and the [upstream d2x installation
guide](https://github.com/wiidev/d2x-cios/wiki/How-to-install-d2x-cIOSs-on-your-Wii)
for package preparation.

### Seed input

The seed contains plaintext console key material. Save the output privately.
Builds accept seeds from EUR, USA and JPN consoles only. Other `AREA` values,
such as AUS or KOR, are rejected.
A footer-bearing backup supplies its own keys. For a footerless backup, pass
its matching 1,024-byte sidecar explicitly.

```sh
(umask 077; wii-seed BACKUP > console.seed.yaml)
(umask 077; wii-seed FOOTERLESS_BACKUP --keys keys.bin > console.seed.yaml)
```

Use `--seed -` to pipe plaintext YAML directly into the builder.

```sh
wii-seed BACKUP | wiiform --seed - --recipe recipe.yaml --output prepared-console
```

Earlier seeds from the unreleased format need to be regenerated from their
backups. See the [seed format](docs/migration.md#what-a-seed-contributes).

## Direct commands

```sh
wii-nand install-title INPUT OUTPUT TITLE_ID VERSION
wii-nand install-wad INPUT OUTPUT PACKAGE.wad --sha256 SHA256
wii-nand install-wad INPUT OUTPUT 'https://example.com/package.wad' --sha256 SHA256
wii-nand remove-title INPUT OUTPUT TITLE_ID
wii-nand install-priiloader INPUT OUTPUT --hack fix_480p --hack block_disc_updates
```

`wii-nand install-title` uses the same `WIIFORM_NUS_CACHE` and
`WIIFORM_NUS_OFFLINE` environment variables as `wiiform`.

Each edit requires a new output path. Run `wii-nand <command> --help` for
settings and Priiloader options. `install-wad` takes `--allow-unsigned` for a
community channel. The Priiloader edit uses the pinned defaults. Pass a switch
such as `--stop-disc` or `--white-background` to enable it, and `--hack NAME`
once per hack.
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
