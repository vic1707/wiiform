# Wiiform backlog

## Current state

- Recipe WADs accept local files, inline bytes, and HTTPS URLs with a required
  SHA-256. An exact installed match emits a warning. A conflicting title fails
  the build. Nintendo-signed IOS WADs use the same title entry, including IOS9
  v1034 for the OoT speedrun setup.
- The `hbc` recipe entry installs verified HBC Reborn OHBC releases or a
  hash-pinned custom HBC WAD before Priiloader configuration.
- Wii d2x WADs have explicit slot, base, and revision checks. Stock stub
  replacement is opt-in and limited to verified Nintendo stubs in slots 249
  and 250.
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
- [ ] **Verify a real d2x WAD.** Generate a Wii d2x-v11-beta3 package with
  ModMii. Verify the package hash, slot, base, revision, stock stub replacement,
  and launch with the intended loader. [ModMii instructions](https://modmii.github.io/usage.html)
- [ ] **Test prepared images on the remaining consoles.** Record first-run
  setup, the Wii Menu recovery message, channel launch, and Priiloader behavior
  for the images tested.
- [ ] **Finish distribution.** Run the CI workflow, including the amd64 image
  and the push to the registry, and review the license notices in the images.
- [ ] **Expand console settings and SD presets when a recipe needs them.**
  Date and time, Wi-Fi, and more Priiloader hacks remain candidates.
