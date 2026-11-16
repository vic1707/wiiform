// SPDX-License-Identifier: GPL-2.0-only
#include <BuildOptions.hpp>
#include <Seed.hpp>
#include <tools.h>
#include <QCryptographicHash>
#include <QSet>
#include <cstring>
#include <stdexcept>

namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto same_digest(const QByteArray& raw, const wiiform::Sha256& expected) -> bool {
    const auto actual = QCryptographicHash::hash(raw, QCryptographicHash::Sha256);
    return actual.size() == static_cast<int>(expected.size()) &&
           std::memcmp(actual.constData(), expected.data(), expected.size()) == 0;
  }
}

namespace nand_build {
  auto area(wiiform::Menu menu) -> QString {
    switch (menu) {
      case wiiform::Menu::e43: return "EUR";
      case wiiform::Menu::u43: return "USA";
      case wiiform::Menu::j43: return "JPN";
    }
    throw std::runtime_error("Unsupported System Menu version");
  }

  auto resolve_profile(const QByteArray& keys, const wiiform::ConsoleSettings& source_settings,
                       const wiiform::Sha256& settings_sha256,
                       std::span<const std::uint16_t> bad_blocks, wiiform::Menu menu) -> Profile {
    require(keys.size() == 0x400, "Expected exactly 1024 key bytes");
    const auto source_bytes = seed::encode_settings(source_settings);
    require(same_digest(source_bytes, settings_sha256), "Seed setting.txt digest does not match its fields");

    const auto& model = source_settings.model;
    const auto& area_original = source_settings.area;
    const auto& game_original = source_settings.game;
    const auto& video_original = source_settings.video;
    require((area_original == "EUR" && game_original == "EU" && video_original == "PAL") ||
                (area_original == "USA" && game_original == "US" && video_original == "NTSC") ||
                (area_original == "JPN" && game_original == "JP" && video_original == "NTSC"),
            "Seed region, game and video settings disagree");

    const QStringList model_regions{"EUR)", "USA)", "JPN)", "UAE)"};
    const auto qmodel = QString::fromStdString(model);
    require((qmodel.startsWith("RVL-001(") || qmodel.startsWith("RVL-101(")) &&
                model_regions.contains(qmodel.mid(8)), "Unsupported console model");
    const uchar korean_key[] = KOREAN_KEY;
    require(!keys.contains(QByteArray(reinterpret_cast<const char*>(korean_key), 16)),
            "Korean-key console is unsupported by this stock profile");

    auto target = source_settings;
    const auto target_area = area(menu);
    target.area = target_area.toStdString();
    target.game = menu == wiiform::Menu::e43 ? "EU" : menu == wiiform::Menu::u43 ? "US" : "JP";
    target.video = menu == wiiform::Menu::e43 ? "PAL" : "NTSC";
    Profile profile{target_area, seed::encode_settings(target), {}};
    QSet<quint16> seen;
    for (const auto block : bad_blocks) {
      require(block >= 8 && block < 4080 && !seen.contains(block), "Invalid or duplicate NAND bad block");
      seen.insert(block);
      profile.bad_blocks.append(block);
    }
    return profile;
  }
} // namespace nand_build
