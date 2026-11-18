# Wiiform backlog

## Current state

- Titles are grouped by kind: channel WADs (local files, inline bytes, HTTPS
  URLs or GitHub assets, with a SHA-256), IOS from NUS by slot and version
  (such as IOS9 v1034 for the OoT speedrun setup), and cIOS. An exact
  installed match emits a warning; a conflicting title fails the build.
- The `hbc` recipe entry installs verified HBC Reborn OHBC releases or a
  hash-pinned custom HBC WAD before Priiloader configuration.
- d2x cIOS are built from the d2x release on their NUS base IOS (matching an
  independent build byte for byte) or installed from prebuilt WADs, with
  slot, base and revision checks. Stock stub replacement is opt-in and limited
  to verified Nintendo stubs in slots 249 and 250. IOS come from NUS.
- `nand.bootmii` installs BootMii as IOS from the seed (captured by `wii-seed`
  from the backup), for consoles that can't run BootMii as boot2.
- The `rvloader` recipe entry does what RVLoader v2.1's installer does: the
  boot file and Priiloader settings, the six derived IOS and the VGA flag. An
  independent re-implementation of the installer matched every derived TMD
  and ticket byte for byte.
- A Rust test keeps the editor schema current. Releases are the container
  images built by CI; the publish path has not run yet.

## Open work

- [ ] **Confirm RVLoader on a console.** Restore an image built with
  `rvloader` to a recoverable Wii (BootMii as boot2: the System Menu's own
  IOS80 is replaced) with the release unpacked on USB, once with each
  `patch_out_wifi` value. Open points:
  - Priiloader 0.10.0 instead of the older build RVLoader bundles, and the
    six-byte `main.nfo` (hardware access on, no arguments).
  - The fakesign padding: WiiQt brute-forces a different TMD field than the
    installer (`reserved2` instead of `fill3`) and writes both padding fields
    little-endian, so those bytes differ from a console install while the
    signatures are equally valid. Compare against a NAND captured after the
    upstream installer if the console disagrees.
- [ ] **Verify a built d2x cIOS on a console.** Restore an image with
  `titles.cios` built from d2x v11 beta3 (slot 249, base 56) and launch a game
  with a loader that uses it.
- [ ] **Launch BootMii as IOS from a prepared image.** On salon (no boot2),
  restore an image with `bootmii: {}` and BootMii's SD files and use the
  Homebrew Channel's "Launch BootMii".
- [ ] **Test prepared images on the remaining consoles.** Record first-run
  setup, the Wii Menu recovery message, channel launch, and Priiloader behavior
  for the images tested.
- [ ] **Finish distribution.** Run the CI workflow, including the amd64 image
  and the push to the registry, and review the license notices in the images.
- [ ] **Expand console settings and SD presets when a recipe needs them.**
  Date and time, Wi-Fi, and more Priiloader hacks remain candidates.
