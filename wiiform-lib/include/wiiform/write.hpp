// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <cstdint>
#include <optional>

// See read.hpp for the calling rules. The caller owns every buffer. A request
// is validated before anything is written; if a call fails after that, discard
// the NAND buffer.
namespace wiiform {
  // Replaces the text header of a 1024-byte key footer with the canonical BackupMii
  // one for the console ID in its OTP. Returns whether the header changed.
  auto write_backupmii_header(std::span<std::uint8_t> keys) -> bool;

  // Formats the buffer as an empty filesystem for this console: boot area, bad blocks,
  // key footer, system directories and the System Menu settings for the menu's region.
  // The seed's boot area must be stock: every boot2 copy genuinely signed.
  auto initialize_image(std::span<std::uint8_t> nand, const StockSeed& seed, Menu menu) -> StockIdentity;
  // The same, keeping a modified boot area (such as BootMii as boot2) as the seed has it.
  // It needs redundant boot2 maps.
  auto initialize_image_with_source_boot(std::span<std::uint8_t> nand, const StockSeed& seed,
                                         Menu menu) -> StockIdentity;

  // Installs one exact, Nintendo-signed title version. Without a cache it is
  // downloaded into memory. An identical installed title is left alone; another
  // version is refused.
  auto install_nus_title(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                         TitleVersion title) -> InstalledWad;

  // The WAD installers verify an identical installed title instead of writing it
  // again, and refuse a different one. None of them checks where the bytes came
  // from: compare a pinned hash first when that matters.
  // A Nintendo-signed channel or Wii IOS (slots 3 through 253).
  auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad;
  // A Nintendo-signed channel (downloadable or system), never an IOS.
  auto install_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad;
  // A downloadable channel that need not be signed.
  auto install_unsigned_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad;
  // A Homebrew Channel release, recognized by its title ID.
  auto install_homebrew_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad;
  // A Wii d2x cIOS matching the given slot, base and revision.
  auto install_cios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                    const CiosOptions& options) -> InstalledWad;
  // Installs BootMii as IOS (IOS254) from the parts a seed captured. It replaces
  // Nintendo's IOS254 stub; an identical installed title is left alone.
  auto install_bootmii_ios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> tmd,
                           std::span<const std::uint8_t> ticket, std::span<const std::uint8_t> content)
      -> InstalledWad;
  // Builds a d2x cIOS from its base IOS and installs it like install_cios.
  auto install_built_cios(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                          const CiosBuild& build) -> InstalledWad;
  // Installs one IOS the way RVLoader's installer does: the stock title's shared
  // modules only, version 65535, fakesigned. An identical title is left alone.
  auto install_rvloader_ios(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                            const RvloaderIos& ios) -> InstalledWad;
  // Only the allowlisted Forecast, News, Shop and Photo Channel titles can be removed.
  void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id);

  // Creates SYSCONF; fails if the image already has one. `registrations` is the BT.DINF
  // value (0x461 bytes) that keeps Wiimotes registered, or empty for none.
  void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings,
                             std::span<const std::uint8_t> registrations = {});
  // Creates the Wi-Fi profile file (`config` is its 7004 bytes), with any missing
  // directories above it; fails if the image already has the file.
  void apply_wifi_config(std::span<std::uint8_t> nand, std::span<const std::uint8_t> config);

  // Installs the pinned Priiloader release over a stock System Menu. An image that
  // already has this release is left unchanged.
  void install_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app);
  // Stores the executable (a DOL) that Priiloader's "file" destinations boot, with or
  // without hardware access (AHBPROT). Needs an installed Priiloader. An identical
  // file is left alone.
  void install_priiloader_boot_file(std::span<std::uint8_t> nand, std::span<const std::uint8_t> dol,
                                    bool hardware_access);
  // Creates or removes the empty file that makes RVLoader's bootloader enable VGA output.
  // Needs an installed Priiloader.
  void set_rvloader_vga(std::span<std::uint8_t> nand, bool enabled);
  // Both need an installed Priiloader. Every setting is written, not just changed ones.
  void configure_priiloader(std::span<std::uint8_t> nand, const PriiloaderSettings& settings);
  void set_priiloader_hacks(std::span<std::uint8_t> nand, std::span<const std::uint8_t> hacks_hash_ini,
                            std::span<const Hack> enabled_hacks);
}
