// SPDX-License-Identifier: GPL-2.0-only
#include "wiiform/src/ffi.rs.h"
#include <wiiform/core.hpp>

#include <algorithm>
#include <iterator>
#include <optional>
#include <vector>

namespace wiiform::ffi {
namespace {
  template<class T>
  auto span(const rust::Slice<T>& values) -> std::span<T> {
    return {values.data(), values.size()};
  }

  template<class T>
  auto rust_vec(const std::vector<T>& values) -> rust::Vec<T> {
    rust::Vec<T> result;
    result.reserve(values.size());
    std::copy(values.begin(), values.end(), std::back_inserter(result));
    return result;
  }

  auto native(const ConsoleSettings& settings) -> wiiform::ConsoleSettings {
    return {std::string(settings.area), std::string(settings.model), std::string(settings.dvd),
            std::string(settings.mpch), std::string(settings.code), std::string(settings.serial),
            std::string(settings.video), std::string(settings.game)};
  }

  // The library's seed refers to its settings, so the caller keeps the converted copy alive.
  auto native(const StockSeed& seed, const wiiform::ConsoleSettings& settings) -> wiiform::StockSeed {
    return {span(seed.keys), span(seed.boot_blocks), span(seed.bad_blocks), settings, seed.settings_sha256};
  }

  auto native(const NusCache& cache) -> std::optional<wiiform::NusCache> {
    if (cache.directory.empty()) return std::nullopt;
    return wiiform::NusCache{std::string(cache.directory), cache.offline};
  }

  auto shared(const wiiform::InstalledWad& wad) -> InstalledWad {
    return {wad.title_id, wad.region, wad.version, wad.already_present};
  }
}

auto extract_seed(Bytes nand, Bytes sidecar_keys) -> SeedParts {
  const auto source = wiiform::extract_seed(span(nand), span(sidecar_keys));
  const auto& settings = source.settings;
  return {rust_vec(source.keys), rust_vec(source.boot_blocks), rust_vec(source.bad_blocks),
          ConsoleSettings{settings.area, settings.model, settings.dvd, settings.mpch,
                          settings.code, settings.serial, settings.video, settings.game},
          source.settings_sha256, source.bootmii, source.bootmii_boot2_version, rust_vec(source.bootmii_ios_tmd),
          rust_vec(source.bootmii_ios_ticket), rust_vec(source.bootmii_ios_content),
          rust_vec(source.wifi_config), rust_vec(source.wiimote_registrations)};
}

auto stock_titles(Menu menu) -> rust::Vec<TitleVersion> {
  rust::Vec<TitleVersion> result;
  for (const auto& title : wiiform::stock_titles(menu)) result.push_back({title.id, title.version});
  return result;
}

void verify_stock(Bytes nand, const StockSeed& seed, Menu menu) {
  const auto settings = native(seed.settings);
  wiiform::verify_stock(span(nand), native(seed, settings), menu);
}

void verify_generated_nand(Bytes nand, const StockIdentity& expected) {
  wiiform::verify_generated_nand(span(nand), {expected.boot_area, expected.footer});
}

auto write_backupmii_header(Nand keys) -> bool {
  return wiiform::write_backupmii_header(span(keys));
}

auto initialize_image(Nand nand, const StockSeed& seed, Menu menu) -> StockIdentity {
  const auto settings = native(seed.settings);
  const auto identity = wiiform::initialize_image(span(nand), native(seed, settings), menu);
  return {identity.boot_area, identity.footer};
}

auto initialize_image_with_source_boot(Nand nand, const StockSeed& seed, Menu menu) -> StockIdentity {
  const auto settings = native(seed.settings);
  const auto identity = wiiform::initialize_image_with_source_boot(span(nand), native(seed, settings), menu);
  return {identity.boot_area, identity.footer};
}

auto install_nus_title(Nand nand, const NusCache& cache, TitleVersion title) -> InstalledWad {
  return shared(wiiform::install_nus_title(span(nand), native(cache), {title.id, title.version}));
}

auto install_rvloader_ios(Nand nand, const NusCache& cache, const RvloaderIos& ios) -> InstalledWad {
  return shared(wiiform::install_rvloader_ios(
      span(nand), native(cache),
      {{ios.source.id, ios.source.version}, ios.slot, ios.es_patches, span(ios.wifi_module)}));
}

void set_rvloader_vga(Nand nand, bool enabled) {
  wiiform::set_rvloader_vga(span(nand), enabled);
}

auto install_wad(Nand nand, Bytes wad) -> InstalledWad {
  return shared(wiiform::install_wad(span(nand), span(wad)));
}

auto install_channel(Nand nand, Bytes wad) -> InstalledWad {
  return shared(wiiform::install_channel(span(nand), span(wad)));
}

auto install_unsigned_channel(Nand nand, Bytes wad) -> InstalledWad {
  return shared(wiiform::install_unsigned_channel(span(nand), span(wad)));
}

auto install_homebrew_channel(Nand nand, Bytes wad) -> InstalledWad {
  return shared(wiiform::install_homebrew_channel(span(nand), span(wad)));
}

auto install_cios(Nand nand, Bytes wad, const CiosOptions& options) -> InstalledWad {
  return shared(wiiform::install_cios(span(nand), span(wad),
      {options.slot, options.base, options.revision, options.replace_stock_stub}));
}

auto install_bootmii_ios(Nand nand, Bytes tmd, Bytes ticket, Bytes content) -> InstalledWad {
  return shared(wiiform::install_bootmii_ios(span(nand), span(tmd), span(ticket), span(content)));
}

auto install_built_cios(Nand nand, const NusCache& cache, const CiosBuild& build) -> InstalledWad {
  // The library's build refers to these spans, so they live until the call returns.
  std::vector<wiiform::ContentPatch> patches;
  for (const auto& patch : build.patches)
    patches.push_back({patch.cid, patch.offset, {patch.original.data(), patch.original.size()},
                       {patch.replacement.data(), patch.replacement.size()}});
  std::vector<wiiform::AddedModule> modules;
  for (const auto& module : build.modules)
    modules.push_back({module.cid, module.position, {module.bytes.data(), module.bytes.size()}});
  const auto& options = build.options;
  return shared(wiiform::install_built_cios(
      span(nand), native(cache),
      {build.base_version, {options.slot, options.base, options.revision, options.replace_stock_stub}, patches,
       modules}));
}

void remove_title(Nand nand, std::uint64_t title_id) {
  wiiform::remove_title(span(nand), title_id);
}

void apply_system_settings(Nand nand, const SystemSettings& settings, Bytes registrations) {
  wiiform::apply_system_settings(span(nand), {
      std::string(settings.nickname), settings.language, settings.country, settings.sensor_bar_above,
      settings.widescreen, settings.progressive, settings.pal60}, span(registrations));
}

void apply_wifi_config(Nand nand, Bytes config) {
  wiiform::apply_wifi_config(span(nand), span(config));
}

void install_priiloader(Nand nand, Bytes app) {
  wiiform::install_priiloader(span(nand), span(app));
}

void install_priiloader_boot_file(Nand nand, Bytes dol, bool hardware_access) {
  wiiform::install_priiloader_boot_file(span(nand), span(dol), hardware_access);
}

void configure_priiloader(Nand nand, const PriiloaderSettings& settings) {
  wiiform::configure_priiloader(span(nand), {
      settings.autoboot, settings.return_to, settings.shutdown_to, settings.preferred_mount_point,
      settings.stop_disc, settings.lid_slot_on_error, settings.ignore_shutdown_mode,
      settings.black_background, settings.dump_gecko_text, settings.show_rc_updates});
}

void set_priiloader_hacks(Nand nand, Bytes definitions, rust::Slice<const Hack> enabled_hacks) {
  wiiform::set_priiloader_hacks(span(nand), span(definitions), span(enabled_hacks));
}
}
