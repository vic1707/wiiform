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
- The `rvloader` recipe entry stores RVLoader's bootloader as Priiloader's
  boot file and makes Priiloader start it.
- A Rust test keeps the editor schema current. Releases are the container
  images built by CI; the publish path has not run yet.

## Open work

- [ ] **Confirm the RVLoader boot hook on a console.** Restore an image built
  with `rvloader` to a recoverable Wii with the release unpacked on USB. Open
  points: Priiloader 0.10.0 instead of the older build RVLoader bundles, and
  the six-byte `main.nfo` (hardware access on, no arguments).
- [ ] **RVLoader installer parity.** The rest of the upstream v2.1 installer,
  for portables without a Wi-Fi module:
  - IOS236: IOS36 v3608 with the hash-check, ES_Identify, ISFS-permission and
    version-check patches. IOS240 and IOS241: copies of IOS58 v6176 and IOS80
    v6944. All three get the NoWiFi change; IOS36/58/80 themselves get it
    optionally. The three base IOS come from NUS, which Wiiform already uses.
  - NoWiFi removes the stock WD and WL modules and adds
    `installer/data/wd.bin` as a new shared content.
  - Titles are fakesigned with version 0xFFFF; IOS58 also gets four 0xFF bytes
    at TMD offset 8. WiiQt's `Wad` has `FakeSign`, `SetVersion` and content
    replacement.
  - VGA: an empty `vga.bin` beside the boot file and `IPL.PGS` (plus
    `IPL.E60` on PAL) in SYSCONF.
  [RVLoader installer](https://github.com/Aurelio92/RVLoader/tree/v2.1/installer/source)
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
