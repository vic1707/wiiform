// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include "rust/cxx.h"
#include <wiiform/types.hpp>
#include <array>
#include <cstdint>

// Translates CXX's types to the library's. Nothing here decides anything.
namespace wiiform::ffi {
  struct SystemSettings;
  struct PriiloaderSettings;
  struct StockIdentity;
  struct CiosOptions;
  struct CiosBuild;
  struct InstalledWad;
  struct StockSeed;
  struct SeedParts;
  struct TitleVersion;
  struct NusCache;
  struct RvloaderIos;

  using Bytes = rust::Slice<const std::uint8_t>;
  using Nand = rust::Slice<std::uint8_t>;

  auto extract_seed(Bytes nand, Bytes sidecar_keys) -> SeedParts;
  auto stock_titles(Menu menu) -> rust::Vec<TitleVersion>;
  void verify_stock(Bytes nand, const StockSeed& seed, Menu menu);
  void verify_generated_nand(Bytes nand, const StockIdentity& expected);

  auto write_backupmii_header(Nand keys) -> bool;
  auto initialize_image(Nand nand, const StockSeed& seed, Menu menu) -> StockIdentity;
  auto initialize_image_with_source_boot(Nand nand, const StockSeed& seed, Menu menu) -> StockIdentity;
  auto install_nus_title(Nand nand, const NusCache& cache, TitleVersion title) -> InstalledWad;
  auto install_rvloader_ios(Nand nand, const NusCache& cache, const RvloaderIos& ios) -> InstalledWad;
  void set_rvloader_vga(Nand nand, bool enabled);
  auto install_wad(Nand nand, Bytes wad) -> InstalledWad;
  auto install_channel(Nand nand, Bytes wad) -> InstalledWad;
  auto install_unsigned_channel(Nand nand, Bytes wad) -> InstalledWad;
  auto install_homebrew_channel(Nand nand, Bytes wad) -> InstalledWad;
  auto install_cios(Nand nand, Bytes wad, const CiosOptions& options) -> InstalledWad;
  auto install_built_cios(Nand nand, const NusCache& cache, const CiosBuild& build) -> InstalledWad;
  void remove_title(Nand nand, std::uint64_t title_id);
  void apply_system_settings(Nand nand, const SystemSettings& settings);
  void install_priiloader(Nand nand, Bytes app);
  void install_priiloader_boot_file(Nand nand, Bytes dol, bool hardware_access);
  void configure_priiloader(Nand nand, const PriiloaderSettings& settings);
  void set_priiloader_hacks(Nand nand, Bytes definitions, rust::Slice<const Hack> enabled_hacks);
}
