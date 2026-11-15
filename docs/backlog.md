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
- Wiiform generates the editor schema and builds release archives. The macOS
  archive was tested after relocation. CI defines Linux and macOS build checks.
  A Linux container build is prototyped separately.

## Open work

- [ ] **Install original Homebrew Channel from scratch.** Keep first-class
  Wiiform support pending a reproducible source for the original HBC 1.1.2
  title and its install metadata. HackMii Installer v1.2 provides Wii
  executables. The public HBC source omits the original installer. Verify the
  ticket, TMD, contents, IOS58 requirements, and any auxiliary NAND writes
  against an official install. Reuse Wiiform’s validated title writer and test
  launch on a recoverable Wii. [Official installer](https://bootmii.org/download/)
  · [Public HBC source](https://github.com/fail0verflow/hbc)
- [ ] **Explore first-class RVLoader installation.** Compare NAND captures
  before and after the upstream v2.1 installer. Account for IOS236, IOS240,
  IOS241, the IOS36/58/80 changes, the NoWiFi and VGA options, and the
  Priiloader boot files. Test on a recoverable Wii before exposing an offline
  install. [RVLoader installer](https://github.com/Aurelio92/RVLoader/tree/v2.1/installer/source)
- [ ] **Verify a real d2x WAD.** Generate a Wii d2x-v11-beta3 package with
  ModMii. Verify the package hash, slot, base, revision, stock stub replacement,
  and launch with the intended loader. [ModMii instructions](https://modmii.github.io/usage.html)
- [ ] **Test prepared images on the remaining consoles.** Record first-run
  setup, the Wii Menu recovery message, channel launch, and Priiloader behavior
  for the images tested.
- [ ] **Finish distribution.** Resolve Linux CLI linkage and runtime library
  packaging, publish archives and checksums, and verify a clean install
  through mise.
- [ ] **Expand console settings and SD presets when a recipe needs them.**
  Date and time, Wi-Fi, and more Priiloader hacks remain candidates.
