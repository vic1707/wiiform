// SPDX-License-Identifier: GPL-2.0-only
#include "wiiform/src/ffi.rs.h"
#include <wiiform/core.hpp>

namespace wiiform::ffi {
namespace {
  auto rust_bytes(std::span<const std::uint8_t> bytes) -> rust::Vec<std::uint8_t> {
    rust::Vec<std::uint8_t> result;
    result.reserve(bytes.size());
    for (const auto byte : bytes) result.push_back(byte);
    return result;
  }

  template<class T>
  auto span(const rust::Slice<T>& bytes) -> std::span<T> {
    return {bytes.data(), bytes.size()};
  }

}

auto extract_seed(Bytes nand, Bytes sidecar_keys) -> SeedParts {
  const auto source = read::extract_seed(span(nand), span(sidecar_keys));
  SeedParts result;
  result.keys.reserve(source.keys.size());
  for (const auto byte : source.keys) result.keys.push_back(byte);
  result.boot_blocks.reserve(source.boot_blocks.size());
  for (const auto byte : source.boot_blocks) result.boot_blocks.push_back(byte);
  result.bad_blocks.reserve(source.bad_blocks.size());
  for (const auto block : source.bad_blocks) result.bad_blocks.push_back(block);
  result.settings = ConsoleSettings{
      rust::String(source.settings.area), rust::String(source.settings.model),
      rust::String(source.settings.dvd), rust::String(source.settings.mpch),
      rust::String(source.settings.code), rust::String(source.settings.serial),
      rust::String(source.settings.video), rust::String(source.settings.game)};
  result.settings_sha256 = source.settings_sha256;
  switch (source.bootmii) {
    case wiiform::BootMiiState::detected: result.bootmii = BootMiiState::Detected; break;
    case wiiform::BootMiiState::stock: result.bootmii = BootMiiState::Stock; break;
    case wiiform::BootMiiState::unknown: result.bootmii = BootMiiState::Unknown; break;
  }
  result.footer_header = rust::String(source.footer_header);
  return result;
}

auto create_stock(const StockSeed& source, Nand nand, rust::Str cache,
                  const StockOptions& options) -> StockIdentity {
  const auto& settings = source.settings;
  const wiiform::ConsoleSettings native_settings{
      std::string(settings.area), std::string(settings.model), std::string(settings.dvd),
      std::string(settings.mpch), std::string(settings.code), std::string(settings.serial),
      std::string(settings.video), std::string(settings.game)};
  const wiiform::StockSeed seed{span(source.keys), span(source.boot_blocks), span(source.bad_blocks),
                                native_settings, source.settings_sha256};
  const auto result = core::create_stock(seed, span(nand), std::string(cache),
      static_cast<wiiform::Menu>(options.menu), static_cast<wiiform::BootPolicy>(options.boot_policy),
      options.offline);
  return {result.boot_area, result.original_footer};
}

void apply_system_settings(Nand nand, const SystemSettings& settings) {
  core::apply_system_settings(span(nand), {
      std::string(settings.nickname), static_cast<wiiform::Language>(settings.language),
      settings.country, settings.sensor_bar_above, settings.widescreen,
      settings.progressive, settings.pal60});
}

void install_nus_title(Nand nand, rust::Str cache, std::uint64_t title_id,
                       std::uint16_t version, bool offline) {
  core::install_nus_title(span(nand), std::string(cache), title_id, version, offline);
}

auto install_wad(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                 WadPolicy policy) -> InstalledWad {
  const auto result = core::install_wad(span(nand), span(wad), sha256,
                                       static_cast<wiiform::WadPolicy>(policy));
  return {result.title_id, result.region, result.version, result.already_present};
}

auto install_cios(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                  const CiosOptions& options) -> InstalledWad {
  const auto result = core::install_cios(span(nand), span(wad), sha256,
      {options.slot, options.base, options.revision, options.replace_stock_stub});
  return {result.title_id, result.region, result.version, result.already_present};
}

void remove_title(Nand nand, std::uint64_t title_id) {
  core::remove_title(span(nand), title_id);
}

void apply_priiloader(Nand nand, Bytes app, Bytes definitions,
                     const PriiloaderSettings& settings, Bytes enabled_hacks) {
  const wiiform::PriiloaderSettings overrides{
      static_cast<wiiform::Autoboot>(settings.autoboot),
      static_cast<wiiform::ReturnTo>(settings.return_to),
      static_cast<wiiform::ShutdownTo>(settings.shutdown_to),
      static_cast<wiiform::MountPoint>(settings.preferred_mount_point),
      settings.stop_disc, settings.lid_slot_on_error, settings.ignore_shutdown_mode,
      settings.black_background, settings.dump_gecko_text, settings.show_rc_updates};
  std::vector<wiiform::Hack> requested;
  requested.reserve(enabled_hacks.size());
  for (const auto hack : enabled_hacks) requested.push_back(static_cast<wiiform::Hack>(hack));
  core::apply_priiloader(span(nand), span(app), span(definitions), overrides, requested);
}

auto prepare_backupmii(Nand nand, const StockIdentity& expected) -> rust::Vec<std::uint8_t> {
  const auto keys = core::prepare_backupmii(span(nand), {expected.boot_area, expected.original_footer});
  return rust_bytes(keys);
}
}
