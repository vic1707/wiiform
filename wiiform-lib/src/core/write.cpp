// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"

#include <BuildOptions.hpp>
#include <Display.hpp>
#include <Factory.hpp>
#include <Install.hpp>
#include <Priiloader.hpp>
#include <Remove.hpp>
#include <Seed.hpp>
#include <WorkingImage.hpp>

#include <QCryptographicHash>
#include <QMap>
#include <QTreeWidgetItem>
#include <cstring>
#include <limits>
#include <memory>
#include <stdexcept>

namespace wiiform::core {
namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto qstring(const std::string& value, const char* label) -> QString {
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

  auto digest(std::span<const std::uint8_t> value) -> Sha256 {
    const auto hash = QCryptographicHash::hash(view(value), QCryptographicHash::Sha256);
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

  template<class Enum, std::size_t N>
  auto option_name(Enum value, const std::array<const char*, N>& names) -> QString {
    const auto index = static_cast<std::size_t>(value);
    require(index < names.size(), "Unsupported setting value");
    return QString::fromLatin1(names[index]);
  }

  auto priiloader_settings(const PriiloaderSettings& value) -> QMap<QString, QString> {
    QMap<QString, QString> result;
    const auto toggle = [&](const QString& key, std::optional<bool> state) {
      if (state) result.insert(key, *state ? "on" : "off");
    };
    if (value.autoboot)
      result["autoboot"] = option_name(*value.autoboot,
          std::array{"disabled", "homebrew_channel", "bootmii_ios", "system_menu", "file"});
    if (value.return_to)
      result["return_to"] = option_name(*value.return_to, std::array{"system_menu", "priiloader", "autoboot", "file"});
    if (value.shutdown_to)
      result["shutdown_to"] = option_name(*value.shutdown_to, std::array{"none", "priiloader", "autoboot"});
    if (value.preferred_mount_point)
      result["preferred_mount_point"] = option_name(*value.preferred_mount_point, std::array{"auto", "sd", "usb"});
    toggle("stop_disc", value.stop_disc);
    toggle("lid_slot_on_error", value.lid_slot_on_error);
    toggle("ignore_shutdown_mode", value.ignore_shutdown_mode);
    toggle("black_background", value.black_background);
    toggle("dump_gecko_text", value.dump_gecko_text);
    toggle("show_rc_updates", value.show_rc_updates);
    return result;
  }

  auto priiloader_hacks(std::span<std::uint8_t> image, std::span<const HackOverride> overrides)
      -> QMap<QString, bool> {
    constexpr std::array names{
        "480p graphics fix in system menu", "Block Online Updates", "Block Disc Updates",
        "Block Disc Autoboot", "Auto-Press A at Health Screen", "Remove Deflicker",
        "Move Disc Channel", "Remove NoCopy Save File Protection", "Replace Health Screen with Backmenu",
        "No System Menu Background Music", "Wiimmfi Patch v4", "Region Free EVERYTHING"};
    QMap<QString, bool> result;
    for (const auto& option : overrides) {
      auto name = option_name(option.hack, names);
      if (option.hack == Hack::region_free_everything) {
        working_image::Image working(image);
        const std::unique_ptr<QTreeWidgetItem> tree(working.nand.GetTree());
        const auto data = working_image::read_authenticated_file(working.nand,
            working_image::find_path(tree.get(), "/title/00000001/00000002/data/setting.txt"), 256);
        if (seed::decode_settings(data)["AREA"].as<std::string>() == "JPN") name += " JP";
      }
      require(!result.contains(name), "Duplicate Priiloader hack override");
      result.insert(name, option.enabled);
    }
    return result;
  }
}

auto create_stock(const StockSeed& seed, std::span<std::uint8_t> nand,
                  const std::string& nus_cache_dir, Menu menu, BootPolicy boot_policy,
                  bool offline) -> StockIdentity {
  detail::ensure_qcore_application();
  const auto cache = qstring(nus_cache_dir, "Cache path must be UTF-8");
  require(nand.size() == nand_size && seed.keys.size() == 0x400 &&
              seed.boot_blocks.size() == 0x108000 && seed.setting_txt.size() == 256,
          "Invalid NAND image or seed component size");
  const StockIdentity identity{digest(seed.boot_blocks), digest(seed.keys)};
  const auto keys = view(seed.keys);
  const auto boots = view(seed.boot_blocks);
  const auto profile = nand_build::resolve_profile(keys, view(seed.setting_txt), seed.bad_blocks, menu);
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
  return identity;
}

void apply_system_settings(std::span<std::uint8_t> nand, const SystemSettings& settings) {
  detail::ensure_qcore_application();
  sysconf::initialize(nand, settings);
}

void install_nus_title(std::span<std::uint8_t> nand, const std::string& nus_cache_dir,
                       std::uint64_t title_id, std::uint16_t version, bool offline) {
  detail::ensure_qcore_application();
  require(nand.size() == nand_size, "Expected a footer-bearing NAND buffer");
  title_install::install_nus_title(nand, qstring(nus_cache_dir, "Cache path must be UTF-8"),
                                   title_id, version, offline);
}

auto install_wad(std::span<std::uint8_t> nand, std::span<const std::uint8_t> wad,
                 const Sha256& expected_sha256, WadPolicy policy) -> InstalledWad {
  detail::ensure_qcore_application();
  const auto [id, region] = title_install::install_wad(nand, wad, expected_sha256, policy);
  return {id, region};
}

void remove_title(std::span<std::uint8_t> nand, std::uint64_t title_id) {
  detail::ensure_qcore_application();
  title_remove::remove_channel(nand, title_id);
}

void apply_priiloader(std::span<std::uint8_t> nand, std::span<const std::uint8_t> app,
                     std::span<const std::uint8_t> hacks_hash_ini, const PriiloaderSettings& settings,
                     std::span<const HackOverride> hacks) {
  detail::ensure_qcore_application();
  priiloader::apply(nand, app, hacks_hash_ini, priiloader_settings(settings),
                    priiloader_hacks(nand, hacks));
}

}
