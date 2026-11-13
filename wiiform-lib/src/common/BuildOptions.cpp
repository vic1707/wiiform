// SPDX-License-Identifier: GPL-2.0-only
#include <BuildOptions.hpp>
#include <Seed.hpp>
#include <tools.h>
#include <QSet>
#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }
}

namespace nand_build {
  auto resolve_profile(const QByteArray& keys, const QByteArray& settings,
                       std::span<const std::uint16_t> bad_blocks, wiiform::Menu menu) -> Profile {
    require(keys.size() == 0x400, "Expected exactly 1024 key bytes");
    auto fields = seed::decode_settings(settings);
    const auto model = QString::fromStdString(fields["MODEL"].as<std::string>());
    const auto area_original = fields["AREA"].as<std::string>();
    const auto game_original = fields["GAME"].as<std::string>();
    const auto video_original = fields["VIDEO"].as<std::string>();
    require((area_original == "EUR" && game_original == "EU" && video_original == "PAL") ||
                (area_original == "USA" && game_original == "US" && video_original == "NTSC") ||
                (area_original == "JPN" && game_original == "JP" && video_original == "NTSC"),
            "Seed region, game and video settings disagree");
    switch (menu) {
      case wiiform::Menu::e43: fields["AREA"] = "EUR"; fields["GAME"] = "EU"; fields["VIDEO"] = "PAL"; break;
      case wiiform::Menu::u43: fields["AREA"] = "USA"; fields["GAME"] = "US"; fields["VIDEO"] = "NTSC"; break;
      case wiiform::Menu::j43: fields["AREA"] = "JPN"; fields["GAME"] = "JP"; fields["VIDEO"] = "NTSC"; break;
      default: throw std::runtime_error("Unsupported System Menu version");
    }
    const auto area = QString::fromStdString(fields["AREA"].as<std::string>());
    const QStringList model_regions{"EUR)", "USA)", "JPN)", "UAE)"};
    require((model.startsWith("RVL-001(") || model.startsWith("RVL-101(")) &&
                model_regions.contains(model.mid(8)), "Unsupported console model");
    const uchar korean_key[] = KOREAN_KEY;
    require(!keys.contains(QByteArray(reinterpret_cast<const char*>(korean_key), 16)),
            "Korean-key console is unsupported by this stock profile");
    Profile profile{area, seed::encode_settings(fields), {}};
    QSet<quint16> seen;
    for (const auto block : bad_blocks) {
      require(block >= 8 && block < 4080 && !seen.contains(block), "Invalid or duplicate NAND bad block");
      seen.insert(block);
      profile.bad_blocks.append(block);
    }
    return profile;
  }
} // namespace nand_build
