// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include "rust/cxx.h"
#include <array>
#include <cstdint>

namespace wiiform::ffi {
  enum class WadPolicy : std::uint8_t;
  struct SystemSettings;
  struct PriiloaderSettings;
  struct HackOverride;
  struct StockOptions;
  struct StockIdentity;
  struct InstalledWad;

  using Bytes = rust::Slice<const std::uint8_t>;
  using Nand = rust::Slice<std::uint8_t>;

  auto extract_seed(Bytes nand, Bytes sidecar_keys) -> rust::String;
  auto create_stock(Bytes keys, Bytes boot_blocks, rust::Slice<const std::uint16_t> bad_blocks,
                    Bytes setting_txt, Nand nand, rust::Str cache, const StockOptions& options) -> StockIdentity;
  void apply_system_settings(Nand nand, const SystemSettings& settings);
  void install_nus_title(Nand nand, rust::Str cache, std::uint64_t title_id,
                         std::uint16_t version, bool offline);
  auto install_wad(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                   WadPolicy policy) -> InstalledWad;
  void remove_title(Nand nand, std::uint64_t title_id);
  void apply_priiloader(Nand nand, Bytes app, Bytes definitions,
                       const PriiloaderSettings& settings, rust::Slice<const HackOverride> hacks);
  auto prepare_backupmii(Nand nand, const StockIdentity& expected) -> rust::Vec<std::uint8_t>;
}
