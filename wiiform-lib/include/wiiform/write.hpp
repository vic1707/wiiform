// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace wiiform::core {
  // The caller owns the buffers. Validation rejections precede writes; discard
  // the working buffer if an exception occurs after a write has started.
  // An empty NUS cache path keeps downloads in memory. Offline requests
  // require a populated cache.
  auto create_stock(const StockSeed& seed, std::span<std::uint8_t> nand,
                    const std::string& nus_cache_dir, Menu menu, BootPolicy boot_policy,
                    bool offline) -> StockIdentity;
  void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings);
  void install_nus_title(std::span<std::uint8_t> nand, const std::string& nus_cache_dir,
                         std::uint64_t title_id, std::uint16_t version, bool offline);
  auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                   const Sha256& expected_sha256, WadPolicy policy) -> InstalledWad;
  void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id);
  void apply_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app,
                         std::span<const std::uint8_t> hacks_hash_ini, const PriiloaderSettings& settings,
                         std::span<const Hack> enabled_hacks);
  auto prepare_backupmii(std::span<std::uint8_t> nand, const StockIdentity& expected)
      -> std::vector<std::uint8_t>;

}
