// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include "rust/cxx.h"
#include <array>
#include <cstdint>

namespace wiiform::ffi {
  enum class WadPolicy : std::uint8_t;
  struct SystemSettings;
  struct PriiloaderSettings;
  struct StockOptions;
  struct StockIdentity;
  struct CiosOptions;
  struct InstalledWad;
  struct StockSeed;
  struct SeedParts;

  using Bytes = rust::Slice<const std::uint8_t>;
  using Nand = rust::Slice<std::uint8_t>;

  auto extract_seed(Bytes nand, Bytes sidecar_keys) -> SeedParts;
  auto create_stock(const StockSeed& seed, Nand nand, rust::Str cache,
                    const StockOptions& options) -> StockIdentity;
  void apply_system_settings(Nand nand, const SystemSettings& settings);
  void install_nus_title(Nand nand, rust::Str cache, std::uint64_t title_id,
                         std::uint16_t version, bool offline);
  auto fetch_https(rust::Str url, std::size_t limit) -> rust::Vec<std::uint8_t>;
  auto install_wad(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                   WadPolicy policy) -> InstalledWad;
  auto install_cios(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                    const CiosOptions& options) -> InstalledWad;
  void remove_title(Nand nand, std::uint64_t title_id);
  void apply_priiloader(Nand nand, Bytes app, Bytes definitions,
                       const PriiloaderSettings& settings, Bytes enabled_hacks);
  auto prepare_backupmii(Nand nand, const StockIdentity& expected) -> rust::Vec<std::uint8_t>;
}
