---
name: wiiform-handoff
description: Resume development or verify changes in the Wiiform Wii NAND builder. Use for a fresh agent handoff, regression checks, real-seed NAND readback, logical comparisons, or release archive smoke tests.
---

# Wiiform handoff

## Orient in the checkout

Use the Wiiform repository as the implementation and start from the revision the
owner selected. Check `git status --short` and `git log -5 --oneline` before
editing. Keep related changes in small, reviewable commits. The reference
revision for this handoff is `e6549f7`.

- Mise pins the toolchain. The host provides Qt with qmake and a C++ toolchain.
  See the build, direct command, and packaging instructions. [README](../../../README.md)
- Check the current inputs and open work before using historical command or
  recipe examples. [Example recipe](../../../docs/recipe.example.yaml) ·
  [Migration notes](../../../docs/migration.md) · [Backlog](../../../docs/backlog.md)
- `wiiform-lib/include/wiiform/` contains the C++ API. The core owns NAND
  validation and writes through WiiQt. `wiiform-cli/src/ffi.rs` and
  `wiiform-cli/src/interop/` are the shared CXX bridge. Rust owns the NAND
  buffer, recipe orchestration, package HTTPS fetches, and output publication.
  Recipe types and schema generation live under `wiiform-cli/recipe/`.

## Choose the checks that match the change

- **Routine regression check.** Run `mise run test` for the baseline and the
  changed revision. This runs Rust formatting, Clippy, Rust tests, native NAND
  and SYSCONF tests, and the schema comparison. The task builds its synthetic
  NAND fixture itself. Plain `cargo test` can skip the fixture-backed near-full
  workflow test. Run `mise run schema` when a recipe shape changes.
- **Seed, writer, or recipe behavior.** Run a prepared build from a real seed
  with the same inputs before and after the change. Verify console identity and
  inspect the intended logical changes. A passing native build authenticates the
  finished filesystem and checks the boot area and key footer before publishing
  the two-file bundle. Compare the output against the recipe and baseline as a
  separate check.
- **Download or packaging behavior.** Exercise the actual download path and a
  failure case if it changed. For runtime or distribution changes, run
  `mise run package` and exercise the relocated archive. The CI workflow builds
  both supported platforms and checks relocation with synthetic inputs.
- **Wii behavior.** Label results as host-verified, RestoreMii simulation, or
  hardware restore and boot, according to what ran. Restore, first-run setup,
  Wii Menu recovery, and app launch need observations from the target Wii.

For a real NAND or archive check, use the recorded real-seed commands and
comparison criteria. [Manual validation](references/manual-validation.md)

## Use adjacent repos when relevant

Routine builds and tests use the Wiiform checkout. When the owner has approved
them, use `wiicli-tools` for its old read-only inventory/comparator, historical
GUI and hardware evidence, or an existing NUS cache and authorized console
inputs. Its seed and recipe examples describe the older implementation.

Use the `wiiqt-tools` fork for allocator, NAND layout, or dependency
investigation. Check the pinned revision and the applied `xmake/patches/`
before drawing conclusions from the fork's current branch. Xmake obtains the
pinned dependency for regular builds. [Xmake pin](../../../xmake/wiiqt-nand.lua)

Use the directories and symlink targets the owner has approved. Keep original
dumps and their sidecars read-only. Keep plaintext seeds, keys, and full
inventories in owner-only local scratch output. Leave age identities and
the owner's homelab presets to the owner unless they give specific permission.
