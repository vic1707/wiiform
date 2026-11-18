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

`mise run build` builds the C++ library with xmake, then the Rust commands with
Cargo, and puts them in `build/bin/`. `mise tasks` lists the tasks; they are
all defined in `mise-tasks.toml`.

```sh
build/bin/wiiform --help
build/bin/wii-nand --help
build/bin/wii-seed --help
```

### Host dependencies

| Provided by | Requirements |
| --- | --- |
| mise | Xmake, CMake, Rust and Cargo, rustfmt, and Clippy |
| Xmake and Cargo | Pinned WiiQt and locked Rust crates |
| Host | Git, a C++20 compiler and linker, make, and a Qt SDK with qmake, Qt Core, and Qt Network |

On macOS, install Xcode Command Line Tools and Homebrew's `qt@5`; xmake finds
it by itself. On Debian or Ubuntu, install `build-essential`, `git`,
`qtbase5-dev`, and `qt5-qmake`, then run `QT_ROOT_DIR=/usr mise run build`.
Set `QT_ROOT_DIR` for any other SDK location.
A first build retrieves dependencies if the caches are empty.

## Recipe build

A recipe has two keys: `nand`, the image to build, and the optional `sd`, files
for the SD card. The NAND keys described below (`menu`, `setup`, `titles`,
`hbc`, `priiloader`, `rvloader`, ...) all sit under `nand`. See the
[example recipe](docs/recipe.example.yaml) and its
[editor schema](docs/wiiform.recipe.schema.json). Run `mise run schema` to
regenerate the schema from the Rust recipe model; the tests fail when it is stale. A WAD entry names exactly
one source next to its `sha256`: `path` for a local file (relative to the
recipe), `url` for an HTTPS download, `inline` for the file as base64 text, or
`github: {repo, tag, asset}` for a release asset.
The SHA-256 is of the raw WAD bytes. Reading a recipe loads, downloads and
checks every WAD it names, before the build starts.
The recipe supports 4.3E, 4.3U, and 4.3J, along with allowlisted
channel removal, initial setup, display settings, and Priiloader settings and named hacks. Set setup country
codes using uppercase ISO alpha-2 values such as `GB`, `US`, and `JP`.

URL sources work for channel and cIOS WADs alike. Downloads use system TLS
trust, follow up to five HTTPS redirects, and have a 60-second total
timeout and the same size limit as a local WAD. Use a local `path` or
`inline` for a network-free build. `WIIFORM_NUS_OFFLINE` controls only
Nintendo title downloads.

WiiQt's own diagnostic messages are hidden; set `WIIFORM_QT_MESSAGES=1` to see
them when investigating a failure.

`titles` groups what gets installed by kind:

- **`channels`:** WADs of channels, Virtual Console and WiiWare titles and app
  channels. They must be Nintendo-signed, unless `allow_unsigned: true` permits
  an unsigned downloadable channel. IOS WADs are refused here.
- **`ios`:** Nintendo IOS, downloaded from NUS by `slot` and `version`, like
  the stock titles. Stock 4.3 already has every IOS it ships, so `{slot: 9,
  version: 1034}` (the Ocarina of Time dependency) only verifies IOS9.
- **`cios`:** d2x cIOS, built or prebuilt; see [cIOS](#cios).

Every title is installed when absent. An exact installed match is verified and
reported with a warning; a different installed title or an incomplete one
stops the build. `wii-nand install-wad` writes a byte-identical copy to the
requested output when it verifies a match, and still takes any WAD, IOS
included.

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

The output must be a new directory. A successful build publishes it whole:
`sd/` holds `nand.bin` and its matching BootMii `keys.bin` (where BootMii
restores from) together with the recipe's `sd` files, and `usb/` holds the
recipe's `usb` files when it has any. Copy the contents of `sd/` to the card's
root and those of `usb/` to the drive's. A `setup` mapping completes
first-run setup using your values and defaults for omitted fields. Put display
options such as the aspect ratio under `setup.display`. Use `setup: {}` to
apply all defaults. Omit `setup` or set it to `null` to run the Wii setup wizard.
Pair Wii Remotes
again after restoring a prepared image.

The commands carry the pinned Priiloader app and hack definitions.
Use `priiloader: {}` to install with pinned defaults.
Settings sit directly under `priiloader`, and `hacks` lists the hacks to
enable. Each application replaces the settings and disables unlisted hacks.
Every Wii hack Priiloader 0.10.0 defines for 4.3 is available; the schema and
`wii-nand install-priiloader --help` describe each, following Priiloader's
[hack list](https://github.com/DacoTaco/priiloader/blob/master/docs/HACKSLIST.md).
Hacks that cancel each other (the two music hacks, the two USB Gecko ones, the
four "run disc games under IOS" ones) cannot be combined.

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
    path: wads/my-homebrew-channel.wad
    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
```

A custom WAD requires its SHA-256. The source may be a recipe-relative path,
inline base64, or an HTTPS URL. Wiiform validates the package and checks its
title ID against the Homebrew Channel IDs recognized by Priiloader before
writing it. The title installs before Priiloader settings are applied, so
`autoboot: homebrew_channel` can use it.

### BootMii

BootMii runs from the SD card's `/bootmii/` folder (`armboot.bin` and
`ppcboot.elf`) and starts in one of two ways:

- **As boot2**, on consoles that allow it: at power-on. It lives in the boot
  area, which `--boot-policy preserve-source` keeps as the backup has it.
- **As IOS** (IOS254): when the Homebrew Channel ("Launch BootMii") or
  Priiloader (`autoboot: bootmii_ios`, or its menu) starts it. This is the only
  way into BootMii on consoles that can't run it as boot2.

```yaml
nand:
  bootmii: {}
sd:
  bundles:
    - url: https://example.com/bootmii-sd-files.zip # /bootmii/armboot.bin and ppcboot.elf
      sha256: "…"
      unzip: /
```

`bootmii: {}` installs BootMii as IOS from the seed:

- **Where it comes from:** `wii-seed` captures it from the backup when the
  HackMii Installer put it there (it does so on every console it sets up, boot2
  ones included). There is no other source: it isn't on NUS, and the HackMii
  Installer may not be redistributed.
- **Seeds:** take the backup after running HackMii and regenerate the seed;
  re-running HackMii updates the IOS.
- **Versions:** any version is accepted. Like the Homebrew Channel, wiiform
  recognizes BootMii by the `BM` marker in its TMD; your consoles show v31338
  and v65281.
- **Checks:** the seed's `bootmii` section records what the backup showed:
  `boot2` (`detected`, `stock` or `unknown`) and `ios` (BootMii as IOS with its
  version, or `null`). With `bootmii`:
  - BootMii as IOS in the seed: installed, with an `info:` line if the console
    also runs BootMii as boot2.
  - None, but BootMii as boot2: a warning, and the build goes on without it
    (boot2 still reaches BootMii; a system update can delete the IOS).
  - Neither: the build stops, since BootMii would be unreachable.
  - A seed from before this support: the build stops and asks to regenerate it.
- **Without the key:** wiiform leaves BootMii alone. The image keeps Nintendo's
  IOS254 stub, so a recipe can return a console to stock.
- **SD files:** these stay an explicit `sd` bundle.

### RVLoader

```yaml
setup: {display: {progressive: true, pal60: true}} # Needed only for vga.
priiloader: {}
rvloader:
  version: "2.1"
  sha256: "30be3599e13f91bba0d5060a316d13bb428ea4d71ba7b35ebe89682cdedd94d7"
  wd_sha256: "6b7f6c95ec886899b43dcb5ca3dc15e20bbaa7c8c4e617e21166876f01cdb7b8"
  patch_out_wifi: true
  vga: true
```

`rvloader` does offline what RVLoader's Wii-side installer does:

- Wiiform downloads two files from the release tag, `installer/data/bootloader.dol`
  and `installer/data/wd.bin` (the replacement Wi-Fi driver), and checks them
  against `sha256` and `wd_sha256`. The hashes above are those of 2.1.
- The bootloader becomes Priiloader's boot file, started with hardware access.
  Priiloader's `autoboot` is set to that file and `return_to` to autoboot.
- IOS236, IOS240 and IOS241 receive copies of IOS36, IOS58 and IOS80 without
  the stock Wi-Fi modules. IOS236's kernel is patched: fakesigning,
  ES_Identify, ISFS permissions and downgrades.
- IOS36, IOS58 and IOS80 are reinstalled the same way, without Wi-Fi only with
  `patch_out_wifi` (the installer's "Patch out WiFi").
- Every one of these IOS keeps only the stock title's shared modules, gets
  version 65535 and is fakesigned, as the installer does on a console. The
  stock IOS come from NUS, like the other Nintendo titles.
- `vga` is the installer's "Enable VGA": it writes the flag file the bootloader
  looks for. It requires `setup.display.progressive`, and `pal60` on 4.3E,
  because the installer turns those on. Component video stops working.

RVLoader is started by Priiloader, so `rvloader` requires a `priiloader`
section without `autoboot` and `return_to`; the other Priiloader settings and
`hacks` apply as usual. The bootloader loads `apps/RVLoader/boot.dol` from a
USB drive: unpack the matching
[RVLoader release](https://github.com/Aurelio92/RVLoader/releases) onto the
drive. Wiiform writes the NAND only.

RVLoader and the Wii Menu keep separate titles. RVLoader lists the WADs in the
drive's `vc/` and `channels/` folders (GameCube games in `games/`, Wii discs in
`wbfs/`) and installs a WAD into an emulated NAND on the drive
(`rvloader/Hiidra/emunand/`) when it first launches it. Titles from the
recipe's `titles` stay on the console's NAND: they appear in the Wii Menu,
which RVLoader opens from its house button, but not in RVLoader's lists.

On an existing image, `wii-nand install-priiloader IN OUT --rvloader 2.1
--rvloader-sha256 HASH --rvloader-wd-sha256 HASH [--patch-out-wifi] [--vga]`
does the same. Its `--vga` writes only the flag: the image's settings must
already enable 480p. Re-running it with the same options leaves the image
unchanged; changing `--patch-out-wifi` on an image that already has the RVLoader
IOS fails, because IOS36, IOS58 and IOS80 are no longer stock.

Booting RVLoader from a prepared image has not yet been confirmed on a console.

### SD card and USB drive

```yaml
sd:
  apps: # Each key is the folder apps/<key>/.
    usbloader_gx:
      github: {repo: wiidev/usbloadergx, tag: v4.0-r1283, asset: usbloadergx_r1283.zip}
    riivolution:
      url: https://aerialx.github.io/rvlution.net/riivolution.zip
      sha256: "…"
    Nintendont:
      files:
        boot.dol: {url: "https://raw.githubusercontent.com/FIX94/Nintendont/COMMIT/loader/loader.dol", sha256: "…"}
  bundles:
    - github: {repo: Fledge68/WiiFlow_Lite, tag: v5.6.2, asset: wiiflow_v5.6.2.zip}
      unzip: /
usb:
  bundles:
    - github: {repo: Aurelio92/RVLoader, tag: v2.1, asset: RVLoader_v2.1.zip}
      sha256: "…"
      extract:
        "RVLoader_v2.1/apps/RVLoader/**": /apps/RVLoader/
        "RVLoader_v2.1/rvloader/**": /rvloader/
```

`sd` and `usb` take the same two keys. Every file names one source and its
SHA-256: `path`, `url` and `inline` work as for WADs, and `github` takes a
release asset by `repo`, `tag` and exact `asset` name. `sha256` may be left out
only for a GitHub asset whose release states one (GitHub records it for assets
uploaded since June 2025); a given `sha256` always wins.

- **`apps`:** each key is the app's folder, `apps/<key>/`.
  - **From an archive:** the archive's one folder holding `boot.dol` or
    `boot.elf` becomes `apps/<key>/`, wherever it sits in the archive
    (`apps/<name>/`, `<name>/`, or the root). An archive with several such
    folders, or with files outside it, needs `extract`.
  - **With `extract`:** glob → directory pairs choose the files, with
    directories relative to the app folder.
  - **From loose files:** `files` maps each file name to its own source and
    SHA-256.
  - **Either way,** the app must end up with a `boot.dol` or `boot.elf`.
- **`bundles`:** anything else, placed from the root: `unzip: DIR` unpacks the
  whole archive there, `extract` unpacks the files its globs choose, and
  `to: PATH` copies a single file.
- **`extract`:** each key is a glob over the archive's paths (`*`, `**`, `?`,
  `[…]`, `{a,b}`), each value a directory. A matching file loses the folders
  the glob names before its `**` (all of them when it has none) and keeps the
  rest: with `"*/data/**": /data/`, `pkg-1.0/data/x/y` lands at `/data/x/y`,
  so a versioned top folder needs no version in the glob. Files no glob matches are left out, as
  RVLoader's bundled BBLoader and old Nintendont are above. A glob matching no
  file, or a file matching two globs, stops the build.
- **Archives:** archives are zips. macOS `__MACOSX/` and `._*` entries are
  skipped; other entries must be plain files with relative paths.
- **Conflicts:** paths are compared ignoring case, as FAT does. Two files with
  different bytes at one path, a path used as both a file and a directory, or
  a root `nand.bin` or `keys.bin` on the card stop the build.

All files are downloaded and checked when the recipe is read, before the NAND
build. Open Shop Channel only serves the latest version of each app, so recipes
pin releases on GitHub (or a URL) instead. mega.nz links cannot be downloaded
(their key is in the URL fragment and the file is decrypted in the browser):
mirror the file and use `path`, `inline` or another URL.

Format cards and drives MBR with one FAT32 partition: libfat, which Wii
homebrew uses to read them, does not read GPT partition tables or exFAT.
RVLoader cannot run from the card; it needs its release on a USB drive.

### cIOS

```yaml
titles:
  cios:
    - d2x:
        github: {repo: wiidev/d2x-cios, tag: d2x-v11-beta3, asset: d2x-v11-beta3.zip}
        sha256: "c2c7c9c5dad247c48f561a2b08f74359227e09271104dec814dbab4ad97e68d3"
      target: {slot: 249, base: 56, revision: 65535, replace_stock_stub: true}
    - path: wads/d2x-250-base57.wad # or a prebuilt cIOS WAD
      sha256: "…"
      target: {slot: 250, base: 57, revision: 65535}
```

A `cios` entry gives the target slot, base IOS and revision, and either a
prebuilt cIOS WAD (source keys and `sha256`, as for channels) or `d2x`, a d2x
release archive with its own source and `sha256`.

- **Built from `d2x`:** wiiform reads the release's `ciosmaps.xml` and
  modules, downloads the base IOS version the map names from NUS (with the
  NUS cache), applies the map's patches and modules, sets the slot and
  revision, and fakesigns the result. That's what d2x's installer does on the
  console and what WiiPy's `ciosbuild` does on a computer; a d2x v11 beta3
  build matched an independent build byte for byte.
- **Prebuilt:** wiiform checks the WAD's TMD, ticket, decrypted content hashes
  and slot and revision.
- **Either way:** the d2x IOSINFO record must name the requested base.

Slots 248–251 and bases 38, 56, 57, and 58 are supported. An empty slot can be
installed, and an exact installed match is verified and reported. Set
`replace_stock_stub: true` to replace the verified Nintendo v65280 stub in
slot 249 or 250. Other occupied slots only accept an exact match, and an `ios`
and a `cios` cannot share a slot.

```sh
wii-nand install-cios INPUT_NAND OUTPUT_NAND FILE --sha256 HEX [--d2x] \
  --slot 249 --base 56 --revision 65535 --replace-stock-stub
```

`FILE` is a WAD path or HTTPS URL, or with `--d2x` the d2x release to build
from (the NUS cache settings apply).

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

`--recipe -` reads the recipe from stdin instead, for a recipe another tool
produces, such as one assembled with `yq`. WAD paths in it are then relative to
the working directory. Only one of the two can be `-`, but either can also be a
pipe: with process substitution an encrypted seed never touches the disk.

```sh
cat wads.yaml recipe.yaml | yq 'explode(.) | del(."x-WAD")' \
  | wiiform --seed <(age -d console.seed.yaml.age) --recipe - --output prepared-console
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

## Container images

Wiiform is released as container images, one per command. `mise run image`
(`docker buildx bake`) builds all three; with podman, build one target at a
time.

```sh
podman build --target wiiform -t wiiform .
podman build --target wii-seed -t wii-seed .
podman build --target wii-nand -t wii-nand .
```

Set `--build-arg BUILD_JOBS=N` to match the memory of the build host; the
default of 2 fits a 2 GiB machine. The images run as an unprivileged user, so
mount an output directory that user can write to.

```sh
podman run --rm -v "$PWD/backups:/in:ro" wii-seed /in/nand.bin |
    podman run --rm -i -v "$PWD/work:/work" wiiform \
        --seed - --recipe /work/recipe.yaml --output /work/bundle
```

CI builds the images for amd64 and arm64 and publishes them to the GitHub
container registry from `main` and from tags.

## License and attribution

Wiiform and migrated code are distributed under GPL-2.0-only. See
[`LICENSE`](LICENSE) and [`THIRD_PARTY.md`](THIRD_PARTY.md) for
the license and upstream attribution record.
