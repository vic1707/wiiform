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
  auto initialize_image(std::span<std::uint8_t> nand, const StockSeed& seed, Menu menu,
                        BootPolicy boot_policy) -> StockIdentity;

  // Installs one exact, Nintendo-signed title version. Without a cache it is
  // downloaded into memory.
  void install_nus_title(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                         TitleVersion title);
  auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                   const Sha256& expected_sha256, WadPolicy policy) -> InstalledWad;
  auto install_cios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                    const Sha256& expected_sha256, const CiosOptions& options) -> InstalledWad;
  // Only the allowlisted Forecast, News, Shop and Photo Channel titles can be removed.
  void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id);

  // Creates SYSCONF; fails if the image already has one.
  void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings);

  // Installs the pinned Priiloader release over a stock System Menu. An image that
  // already has this release is left unchanged.
  void install_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app);
  // Both need an installed Priiloader. Every setting is written, not just changed ones.
  void configure_priiloader(std::span<std::uint8_t> nand, const PriiloaderSettings& settings);
  void set_priiloader_hacks(std::span<std::uint8_t> nand, std::span<const std::uint8_t> hacks_hash_ini,
                            std::span<const Hack> enabled_hacks);
}
