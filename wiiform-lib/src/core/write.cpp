// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"

#include <BuildOptions.hpp>
#include <Display.hpp>
#include <Factory.hpp>
#include <Install.hpp>
#include <Inspection.hpp>
#include <Layout.hpp>
#include <NandBuffer.hpp>
#include <Priiloader.hpp>
#include <Remove.hpp>

#include <QCryptographicHash>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string_view>

namespace wiiform::core {
namespace {
  constexpr std::size_t boot_area_size = 0x108000;
  constexpr std::size_t footer_size = nand_size - raw_nand_size;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

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

  auto digest(const QByteArray& value) -> Sha256 {
    const auto hash = QCryptographicHash::hash(value, QCryptographicHash::Sha256);
    Sha256 result;
    std::memcpy(result.data(), hash.constData(), result.size());
    return result;
  }

  auto native_boot_policy(BootPolicy value) -> inspect::BootPolicy {
    switch (value) {
      case BootPolicy::stock: return inspect::BootPolicy::Stock;
      case BootPolicy::preserve_source: return inspect::BootPolicy::PreserveSource;
    }
    throw std::runtime_error("Unsupported boot policy");
  }
}

auto create_stock(const StockSeed& seed, std::span<std::uint8_t> nand,
                  const std::string& nus_cache_dir, Menu menu, BootPolicy boot_policy,
                  bool offline) -> StockIdentity {
  const detail::ApiCall call;
  const auto cache = qstring(nus_cache_dir, "Cache path must be UTF-8");
  require(nand.size() == nand_size && seed.keys.size() == footer_size &&
              seed.boot_blocks.size() == boot_area_size,
          "Invalid NAND image or seed component size");

  constexpr int header_size = 0x100;
  constexpr int otp_console_id_offset = 0x124;
  auto keys = view(seed.keys);
  const auto console_id = keys.mid(otp_console_id_offset, 4).toHex();
  const auto header = ("BackupMii v1, ConsoleID: " + console_id + '\n').leftJustified(header_size, '\0');
  const bool header_replaced = keys.left(header_size) != header;
  if (header_replaced) keys.replace(0, header_size, header);

  const auto boots = view(seed.boot_blocks);
  const StockIdentity identity{digest(boots), digest(keys), header_replaced};
  const auto profile = nand_build::resolve_profile(keys, seed.settings, seed.settings_sha256,
                                                     seed.bad_blocks, menu);
  nand_build::initialize_image(nand, keys, boots, profile, native_boot_policy(boot_policy));
  const auto titles = factory::stock_titles(profile.area);
  for (auto title = titles.cbegin(); title != titles.cend(); ++title)
    title_install::install_nus_title(nand, cache, title.key(), title.value(), offline);

  const auto layout = inspect::validate_layout(nand, true);
  nand_buffer::Device device{std::span<const std::uint8_t>(nand)};
  NandBin reopened;
  require(reopened.SetDevice(&device) && reopened.InitNand() && reopened.CheckHmacMeta(layout.active_metadata),
          "Cannot authenticate the stock NAND");
  inspect::verify_filesystem(reopened, true, layout.active_metadata);
  inspect::verify_stock(reopened, profile.area);
  require(reopened.GetData("/title/00000001/00000002/data/setting.txt") == profile.settings,
          "Generated System Menu settings do not match the validated seed");
  return identity;
}

void verify_generated_nand(std::span<const std::uint8_t> nand, const StockIdentity& expected) {
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  require(digest(view(nand.first(boot_area_size))) == expected.boot_area,
          "NAND boot area differs from the verified seed");
  require(digest(view(nand.last(footer_size))) == expected.footer,
          "NAND key footer differs from the stock build");

  const detail::ApiCall call;
  const auto layout = inspect::validate_layout(nand, true);
  nand_buffer::Device device{nand};
  NandBin reopened;
  require(reopened.SetDevice(&device) && reopened.InitNand() && reopened.CheckHmacMeta(layout.active_metadata),
          "Cannot authenticate the final NAND filesystem");
  inspect::verify_filesystem(reopened, true, layout.active_metadata);
}

void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings) {
  const detail::ApiCall call;
  wii_sysconf::initialize(nand, settings);
}

void install_nus_title(std::span<std::uint8_t> nand, const std::string& nus_cache_dir,
                       std::uint64_t title_id, std::uint16_t version, bool offline) {
  const detail::ApiCall call;
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  title_install::install_nus_title(nand, qstring(nus_cache_dir, "Cache path must be UTF-8"),
                                   title_id, version, offline);
}

auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                 const Sha256& expected_sha256, WadPolicy policy) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_wad(nand, wad, expected_sha256, policy);
}

auto install_cios(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                  const Sha256& expected_sha256, const CiosOptions& options) -> InstalledWad {
  const detail::ApiCall call;
  return title_install::install_cios(nand, wad, expected_sha256, options);
}

void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id) {
  const detail::ApiCall call;
  title_remove::remove_channel(nand, title_id);
}

void apply_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app,
                     std::span<const std::uint8_t> hacks_hash_ini, const PriiloaderSettings& settings,
                     std::span<const Hack> enabled_hacks) {
  const detail::ApiCall call;
  priiloader::apply(nand, app, hacks_hash_ini, settings, enabled_hacks);
}

}
