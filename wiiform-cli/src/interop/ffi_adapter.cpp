// SPDX-License-Identifier: GPL-2.0-only
#include "wiiform/src/ffi.rs.h"
#include <wiiform/core.hpp>
#include <stdexcept>

namespace wiiform::ffi {
namespace {
  template<class T>
  auto span(const rust::Slice<T>& bytes) -> std::span<T> {
    return {bytes.data(), bytes.size()};
  }

  auto toggle(Toggle value) -> std::optional<bool> {
    switch (value) {
      case Toggle::Unset: return std::nullopt;
      case Toggle::Off: return false;
      case Toggle::On: return true;
    }
    throw std::runtime_error("Unsupported toggle");
  }

  template<class Output, class Input>
  auto option(Input value) -> std::optional<Output> {
    if (value == Input::Unset) return std::nullopt;
    return static_cast<Output>(value);
  }
}

auto extract_seed(Bytes nand, Bytes sidecar_keys) -> rust::String {
  return rust::String(read::extract_seed(span(nand), span(sidecar_keys)));
}

auto create_stock(Bytes keys, Bytes boot_blocks, rust::Slice<const std::uint16_t> bad_blocks,
                  Bytes setting_txt, Nand nand, rust::Str cache, const StockOptions& options) -> StockIdentity {
  const wiiform::StockSeed seed{span(keys), span(boot_blocks), span(bad_blocks), span(setting_txt)};
  const auto result = core::create_stock(seed, span(nand), std::string(cache),
      static_cast<wiiform::Menu>(options.menu), static_cast<wiiform::BootPolicy>(options.boot_policy),
      static_cast<wiiform::CacheMode>(options.cache_mode));
  return {result.boot_area, result.original_footer};
}

void apply_system_settings(Nand nand, const SystemSettings& settings) {
  core::apply_system_settings(span(nand), {
      std::string(settings.nickname), option<wiiform::Language>(settings.language),
      settings.country ? std::optional(settings.country) : std::nullopt,
      toggle(settings.sensor_bar_above), toggle(settings.widescreen),
      toggle(settings.progressive), toggle(settings.pal60)});
}

void install_nus_title(Nand nand, rust::Str cache, std::uint64_t title_id,
                       std::uint16_t version, CacheMode mode) {
  core::install_nus_title(span(nand), std::string(cache), title_id, version,
                          static_cast<wiiform::CacheMode>(mode));
}

auto install_wad(Nand nand, Bytes wad, const std::array<std::uint8_t, 32>& sha256,
                 WadPolicy policy) -> InstalledWad {
  const auto result = core::install_wad(span(nand), span(wad), sha256,
                                       static_cast<wiiform::WadPolicy>(policy));
  return {result.title_id, result.region};
}

void remove_title(Nand nand, std::uint64_t title_id) {
  core::remove_title(span(nand), title_id);
}

void apply_priiloader(Nand nand, Bytes app, Bytes definitions,
                     const PriiloaderSettings& settings, rust::Slice<const HackOverride> hacks) {
  const wiiform::PriiloaderSettings overrides{
      option<wiiform::Autoboot>(settings.autoboot), option<wiiform::ReturnTo>(settings.return_to),
      option<wiiform::ShutdownTo>(settings.shutdown_to), option<wiiform::MountPoint>(settings.preferred_mount_point),
      toggle(settings.stop_disc), toggle(settings.lid_slot_on_error), toggle(settings.ignore_shutdown_mode),
      toggle(settings.black_background), toggle(settings.dump_gecko_text), toggle(settings.show_rc_updates)};
  std::vector<wiiform::HackOverride> requested;
  requested.reserve(hacks.size());
  for (const auto& hack : hacks) requested.push_back({static_cast<wiiform::Hack>(hack.hack), hack.enabled});
  core::apply_priiloader(span(nand), span(app), span(definitions), overrides, requested);
}

auto prepare_backupmii(Nand nand, const StockIdentity& expected) -> rust::Vec<std::uint8_t> {
  const auto keys = core::prepare_backupmii(span(nand), {expected.boot_area, expected.original_footer});
  rust::Vec<std::uint8_t> result;
  result.reserve(keys.size());
  for (const auto key : keys) result.push_back(key);
  return result;
}
}
