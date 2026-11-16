# Manual Wiiform validation

`mise run test` covers generated data and failure handling. Use the checks
below when touching an operation whose real inputs, runtime, or Wii behavior
matter. Keep the inputs fixed when comparing two revisions.

## Build from a real seed

Use the current `wii-seed` for a current-format seed. A footer-bearing
backup takes one positional argument. A footerless backup requires
`--keys "$source_keys"` with its matching 1,024-byte sidecar. When testing
extraction, check both forms and an incorrect sidecar or argument combination.
Redirect seed output to a private file or pipe it straight into
`wiiform --seed -`.

Set `backup` and `nus_cache` to approved absolute paths and run these commands
from Wiiform in one Bash session so the later blocks reuse its variables.
This case uses the parents 4.3E capture with BootMii in
boot2, so it selects `preserve-source`. Use the default stock policy for an
appropriate stock-boot capture.

```bash
set -euo pipefail
: "${backup:?set backup to a footer-bearing Wii backup}"
: "${nus_cache:?set nus_cache to a populated cache}"
umask 077
mkdir -p build/tmp
scratch=$(mktemp -d "$PWD/build/tmp/wiiform-check.XXXXXX")

mise run build
export WIIFORM_NUS_CACHE="$nus_cache" WIIFORM_NUS_OFFLINE=1

build/bin/wii-seed "$backup" > "$scratch/console.seed.yaml"
cat > "$scratch/recipe.yaml" <<'YAML'
menu: 4.3E
remove: [forecast-channel, news-channel]
setup: {}
priiloader: {}
YAML

build/bin/wiiform --seed "$scratch/console.seed.yaml" \
  --recipe "$scratch/recipe.yaml" --output "$scratch/prepared" \
  --boot-policy preserve-source
```

Use a recipe with local or inline packages for a network-free test.
`WIIFORM_NUS_OFFLINE=1` controls Nintendo titles only. Reborn's version lookup,
the RVLoader bootloader and URL sources still use HTTPS. Pin package bytes with their SHA-256, including
an explicit checksum for a Reborn version if the asset must stay fixed.
For a frozen HBC input use the same hash-pinned WAD via `hbc.custom`.

## Check a newly generated bundle

Run the following with the same footer-bearing input and scratch directory.
For a footerless input, use the matching sidecar as the source footer.

```bash
python3 - "$backup" "$scratch/prepared" <<'PY'
from pathlib import Path
import sys

source, bundle = map(Path, sys.argv[1:])
image = bundle / "nand.bin"
assert image.stat().st_size == 0x21000400

with source.open("rb") as before, image.open("rb") as after:
    assert before.read(0x108000) == after.read(0x108000), "boot bytes changed"
    before.seek(0x21000000)
    after.seek(0x21000000)
    source_footer, output_footer = before.read(0x400), after.read(0x400)

assert len(source_footer) == len(output_footer) == 0x400
assert (bundle / "keys.bin").read_bytes() == output_footer, "sidecar differs"
assert source_footer[0x100:] == output_footer[0x100:], "binary footer changed"
console_id = output_footer[0x124:0x128].hex().encode("ascii")
header = (b"BackupMii v1, ConsoleID: " + console_id + b"\n").ljust(0x100, b"\0")
assert output_footer[:0x100] == header, "wrong BackupMii header"
print("Boot bytes, binary footer, BackupMii header, and sidecar checked")
PY
```

Confirm that Wiiform warned exactly when the source header differed from the
canonical header. Re-extract the generated NAND with `wii-seed` if testing
that it opens as a future seed source. Keep its stdout private.

For broader checks, use the legacy tools below. Run
`"$reference/build/bin/wii-nand" inspect-sysconf NAND` for decoded setup values,
or `"$reference/build/bin/wii-nand" inspect-priiloader NAND` for the loader's
boot layout, settings, and hack files. Keep those JSON reports local. For a
stock baseline, use a recipe containing just
`menu: 4.3E` and the same seed, cache, and boot policy. Compare it with the
prepared image to check channel removals and added titles against the recipe.
For WAD or cIOS changes, exercise the real package and the applicable
reapply, conflict, or rejection case.
For example, the saved IOS9 v1034 WAD should match the copy already in stock
4.3E. A standalone `install-wad` exact match produces a byte-identical
output. A rejected build with a fresh output path should leave that path
absent.

## Compare repeat builds or revisions

Use the same seed, recipe, packages, NUS cache, and Priiloader assets. Build
into a second fresh directory, for example `"$scratch/repeat"`. Compare two
builds of one revision first if investigating nondeterminism. For a regression,
build the baseline and candidate in separate, authorized checkout-local
worktrees with their own `build/` directories. Use the same absolute input
paths and a fresh output path for each revision.

`nand.bin` SHA-256 can differ because the pinned WiiQt allocator randomly
chooses free clusters. Placement, FAT metadata, and the corresponding spare
bytes can differ. Use raw digests to identify artifacts and logical inventories
to judge equality. The boot area, bad-block records, key footer,
authenticated files and their attributes, titles, UID map, and shared content
map should agree on an unchanged repeat.

Set `reference` to an authorized `wiicli-tools` checkout with its own built
`build/bin/wii-nand` and `build/libexec/wii-nand-preflight`. The script uses
these reference binaries for its `inventory` command and can read Wiiform
output. Build those binaries from the old repo's README if needed and in
scope. When Wiiform's staged binaries are the only ones available, report the
built-in authentication and bundle checks and mark logical comparison as
unverified.

```bash
: "${reference:?set reference to the wiicli-tools checkout}"
build/bin/wiiform --seed "$scratch/console.seed.yaml" \
  --recipe "$scratch/recipe.yaml" --output "$scratch/repeat" \
  --boot-policy preserve-source
cmp -s "$scratch/prepared/keys.bin" "$scratch/repeat/keys.bin"
python3 -B "$reference/tests/nand_lab/nand_compare.py" \
  "$scratch/prepared/nand.bin" "$scratch/repeat/nand.bin" "$scratch/comparison"
```

For a repeat, require `comparison_complete: true` and `logical_equal: true`
in `"$scratch/comparison/comparison.json"`. The comparator exits successfully
for a completed comparison even when it finds logical differences. For an
intentional change, account for each logical difference. The legacy reader
shares WiiQt with Wiiform, so this check compares the resulting state and
exercises independent orchestration around the same backend.

## Exercise an image or URL change

Build the images and repeat the prepared build inside them, mounting the
backup and the NUS cache read-only and a writable output directory. See the
[container section](../../../../README.md#container-images).

```bash
podman build --target wii-seed -t wii-seed .
podman build --target wiiform -t wiiform .
podman run --rm -v "$(dirname "$backup"):/in:ro" wii-seed "/in/$(basename "$backup")" |
    podman run --rm -i -v "$scratch:/work" -v "$nus_cache:/cache:ro" \
        -e WIIFORM_NUS_CACHE=/cache -e WIIFORM_NUS_OFFLINE=1 wiiform \
        --seed - --recipe /work/recipe.yaml --output /work/bundle
```

Compare the result with a native build using the logical comparator. Exercise
a real HTTPS payload when working on downloads, and check that a bad SHA-256
fails without publishing a bundle.

## What has actually been checked

- The parents 4.3E repeat with setup, Forecast and News removals, and Priiloader
  had different raw NAND hashes, matching sidecars, and zero logical
  differences. This used the current Wiiform builder and the old comparator.
- The BackupMii header change was exercised with BootMii and NAND Dumper
  captures and a scratch copy with an unrecognized header. The canonical
  BootMii header matched exactly. Both other inputs warned, and all three
  preserved the binary keys and boot area while publishing matching sidecars.
- A relocated macOS archive built Reborn 1.12.1 and 1.16.0 via the real GitHub
  API and asset downloads. An incorrect recipe SHA-256 failed without a bundle.
- The owner restored earlier parents stock and prepared checkpoints. The Menu,
  PAL Ocarina of Time, and Priiloader were observed working. A prepared Menu
  also reported recovering Menu data. For hardware follow-up, record the
  precise image and console, RestoreMii simulation or restore result, first
  boot and wizard behavior, the recovery message, and the features exercised.
  The full historical record is in the old repo's
  `docs/validation-and-release-plan.md`. Use the owner's recovery procedure
  and authorization for any physical write.

## Local inputs and cleanup

In the owner's checkout, recent checks used the parents BootMii capture in
`wiicli-tools/wiis-work-and-nands/wiis/parents/backups/` and the cache at
`wiicli-tools/private/composed-stock-validation-2026-11-10/nus-cache`.
Choose equivalent approved inputs on another machine. The remainder of the
large old `private/` tree mainly holds historical outputs and evidence.
The committed tests generate their own fixtures.

For future manual runs, retaining one known-good prepared recipe and its
approved source backup, cache, and hashed packages is enough to reproduce
this smoke. Record the tested commit, settings or features checked, logical
outcome, and hardware status separately from the private files. Delete only
the scratch directory created for the run after reviewing or saving its
results.
