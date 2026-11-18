// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/read.hpp>
#include <Support.hpp>
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"

#include <BuildOptions.hpp>
#include <SysConfWrite.hpp>
#include <Factory.hpp>
#include <Install.hpp>
#include <WadPackage.hpp>
#include <Inspection.hpp>
#include <Layout.hpp>
#include <NandBuffer.hpp>
#include <Priiloader.hpp>
#include <Remove.hpp>
#include <Seed.hpp>

#include <QCryptographicHash>
#include <algorithm>
#include <array>
#include <cstdio>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string_view>

namespace wiiform {
namespace {
  auto qstring(std::string_view value, const char* label) -> QString {
    require(value.size() <= std::size_t(std::numeric_limits<int>::max()), label);
    const auto converted = QString::fromUtf8(value.data(), int(value.size()));
    require(value.find('\0') == std::string::npos &&
                converted.toUtf8() == QByteArray(value.data(), qsizetype(value.size())), label);
    return converted;
  }

  auto view(std::span<const std::uint8_t> value) -> QByteArray {
    require(value.size() <= std::size_t(std::numeric_limits<int>::max()), "Input exceeds supported size");
    return QByteArray::fromRawData(reinterpret_cast<const char*>(value.data()), int(value.size()));
  }

  auto profile_for(const StockSeed& seed, Menu menu) -> nand_build::Profile {
    require(seed.keys.size() == footer_size && seed.boot_blocks.size() == boot_area_size,
            "Invalid seed component size");
    return nand_build::resolve_profile(view(seed.keys), seed.settings, seed.settings_sha256,
                                       seed.bad_blocks, menu);
  }

  // A read-only view of a complete image whose active metadata authenticates.
  struct Authenticated {
    explicit Authenticated(std::span<const std::uint8_t> nand, const char* failure)
        : layout(inspect::validate_layout(nand, true)), device(nand) {
      require(reopened.SetDevice(&device) && reopened.InitNand() &&
                  reopened.CheckHmacMeta(layout.active_metadata), failure);
      inspect::verify_filesystem(reopened, true, layout.active_metadata);
    }
    inspect::LayoutSummary layout;
    nand_buffer::Device device;
    NandBin reopened;
  };
}

auto extract_seed(std::span<const std::uint8_t> nand,
                  std::span<const std::uint8_t> sidecar_keys) -> SeedParts {
  const detail::ApiCall call;
  return seed::extract(nand, sidecar_keys);
}

auto stock_titles(Menu menu) -> std::vector<TitleVersion> {
  const auto titles = factory::stock_titles(nand_build::area(menu));
  std::vector<TitleVersion> result;
  result.reserve(static_cast<std::size_t>(titles.size()));
  for (auto title = titles.cbegin(); title != titles.cend(); ++title)
    result.push_back({title.key(), title.value()});
  return result;
}

void verify_stock(std::span<const std::uint8_t> nand, const StockSeed& seed, Menu menu) {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  const auto profile = profile_for(seed, menu);
  Authenticated image(nand, "Cannot authenticate the stock NAND");
  inspect::verify_stock(image.reopened, profile.area);
  require(image.reopened.GetData("/title/00000001/00000002/data/setting.txt") == profile.settings,
          "Generated System Menu settings do not match the validated seed");
}

void verify_generated_nand(std::span<const std::uint8_t> nand, const StockIdentity& expected) {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  require(sha256(view(nand.first(boot_area_size))) == expected.boot_area,
          "NAND boot area differs from the verified seed");
  require(sha256(view(nand.last(footer_size))) == expected.footer,
          "NAND key footer differs from the stock build");
  Authenticated image(nand, "Cannot authenticate the final NAND filesystem");
}

auto write_backupmii_header(std::span<std::uint8_t> keys) -> bool {
  constexpr std::size_t header_size = 0x100;
  constexpr std::size_t otp_console_id_offset = 0x124;
  require(keys.size() == footer_size, "Expected exactly 1024 key footer bytes");
  std::array<char, header_size> header{};
  std::snprintf(header.data(), header.size(), "BackupMii v1, ConsoleID: %02x%02x%02x%02x\n",
                keys[otp_console_id_offset], keys[otp_console_id_offset + 1],
                keys[otp_console_id_offset + 2], keys[otp_console_id_offset + 3]);
  if (std::equal(header.begin(), header.end(), keys.begin())) return false;
  std::copy(header.begin(), header.end(), keys.begin());
  return true;
}

namespace {
  // Shared by the two public initializers, which differ in the boot area they accept.
  template<class VerifyBoot>
  auto initialize(std::span<std::uint8_t> nand, const StockSeed& seed, Menu menu,
                  VerifyBoot&& verify_boot) -> StockIdentity {
    const detail::ApiCall call;
    require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
    const auto profile = profile_for(seed, menu);
    const auto keys = view(seed.keys);
    const auto boots = view(seed.boot_blocks);
    verify_boot(boots);
    nand_build::initialize_image(nand, keys, boots, profile);
    return {sha256(boots), sha256(keys)};
  }
}

auto initialize_image(std::span<std::uint8_t> nand, const StockSeed& seed, Menu menu) -> StockIdentity {
  return initialize(nand, seed, menu, inspect::verify_stock_boot);
}

auto initialize_image_with_source_boot(std::span<std::uint8_t> nand, const StockSeed& seed,
                                       Menu menu) -> StockIdentity {
  return initialize(nand, seed, menu, inspect::verify_source_boot);
}

auto install_nus_title(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                       TitleVersion title) -> InstalledWad {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  require(!cache || !cache->directory.empty(), "A NUS cache needs a directory");
  const auto directory = cache ? qstring(cache->directory, "Cache path must be UTF-8") : QString();
  return title_install::install_nus_title(nand, directory, title.id, title.version, cache && cache->offline);
}

auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_wad(nand, wad_package::read(wad));
}

auto install_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_wad(nand, wad_package::read_channel(wad));
}

auto install_bootmii_ios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> tmd,
                         std::span<const std::uint8_t> ticket, std::span<const std::uint8_t> content)
    -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_bootmii_ios(nand, view(tmd), view(ticket), view(content));
}

auto install_built_cios(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                        const CiosBuild& build) -> InstalledWad {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  require(!cache || !cache->directory.empty(), "A NUS cache needs a directory");
  const auto directory = cache ? qstring(cache->directory, "Cache path must be UTF-8") : QString();
  return title_install::install_built_cios(nand, directory, cache && cache->offline, build);
}

auto install_unsigned_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_wad(nand, wad_package::read_unsigned_channel(wad));
}

auto install_homebrew_channel(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_wad(nand, wad_package::read_homebrew_channel(wad));
}

auto install_cios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                  const CiosOptions& options) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_cios(nand, wad, options);
}

auto install_rvloader_ios(std::span<std::uint8_t> nand, const std::optional<NusCache>& cache,
                          const RvloaderIos& ios) -> InstalledWad {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  require(!cache || !cache->directory.empty(), "A NUS cache needs a directory");
  const auto directory = cache ? qstring(cache->directory, "Cache path must be UTF-8") : QString();
  return title_install::install_rvloader_ios(nand, directory, cache && cache->offline, ios);
}

void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id) {
  const detail::ApiCall call;
  title_remove::remove_channel(nand, title_id);
}

void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings) {
  const detail::ApiCall call;
  wii_sysconf::initialize(nand, settings);
}

void install_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app) {
  const detail::ApiCall call;
  priiloader::install(nand, app);
}

void install_priiloader_boot_file(std::span<std::uint8_t> nand, std::span<const std::uint8_t> dol,
                                  bool hardware_access) {
  const detail::ApiCall call;
  priiloader::install_boot_file(nand, dol, hardware_access);
}

void set_rvloader_vga(std::span<std::uint8_t> nand, bool enabled) {
  const detail::ApiCall call;
  priiloader::set_rvloader_vga(nand, enabled);
}

void configure_priiloader(std::span<std::uint8_t> nand, const PriiloaderSettings& settings) {
  const detail::ApiCall call;
  priiloader::configure(nand, settings);
}

void set_priiloader_hacks(std::span<std::uint8_t> nand, std::span<const std::uint8_t> hacks_hash_ini,
                          std::span<const Hack> enabled_hacks) {
  const detail::ApiCall call;
  priiloader::set_hacks(nand, hacks_hash_ini, enabled_hacks);
}

}
