// SPDX-License-Identifier: GPL-2.0-only
#include <BuildOptions.hpp>
#include <Factory.hpp>
#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  bool valid_override(const QString& key, const QString& value) {
    if (key == "AREA") return value == "EUR" || value == "USA" || value == "JPN";
    if (key == "GAME") return value == "EU" || value == "US" || value == "JP";
    if (key == "VIDEO") return value == "PAL" || value == "NTSC";
    return false;
  }
}

namespace nand_build {
  auto boot_policy_name(inspect::BootPolicy policy) -> QString {
    return policy == inspect::BootPolicy::Stock ? "stock" : "preserve-source";
  }

  auto resolve_profile(const seed::Components& seed, const Options& options) -> Profile {
    require(!::seed::has_original_settings(seed.manifest) && seed.files.value("setting.txt").size() == 256,
            "Stock build requires canonical console settings");
    auto fields = YAML::Clone(::seed::settings_fields(seed.manifest));
    const auto model = QString::fromStdString(fields["MODEL"].as<std::string>());
    for (auto field = options.setting_overrides.cbegin(); field != options.setting_overrides.cend(); ++field) {
      require(valid_override(field.key(), field.value()), "Invalid AREA, GAME or VIDEO override");
      fields[field.key().toStdString()] = field.value().toStdString();
    }
    const auto area = QString::fromStdString(fields["AREA"].as<std::string>());
    const QStringList model_regions{"EUR)", "USA)", "JPN)", "UAE)"};
    require((model.startsWith("RVL-001(") || model.startsWith("RVL-101(")) &&
                model_regions.contains(model.mid(8)), "Unsupported console model");
    factory::stock_titles(area);
    const uchar korean_key[] = KOREAN_KEY;
    require(!seed.files.value("keys.bin").contains(QByteArray(reinterpret_cast<const char*>(korean_key), 16)),
            "Korean-key console is unsupported by this stock profile");
    Profile profile{area, seed::encode_settings(fields), {}};
    for (const auto& entry : seed.manifest["bad_blocks"]) {
      const auto block = entry.as<int>();
      require(block < 4080, "Bad metadata block requires separate review");
      profile.bad_blocks.append(static_cast<quint16>(block));
    }
    return profile;
  }
} // namespace nand_build
