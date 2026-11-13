// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QString>
#include <wiiform/types.hpp>
#include <cstdint>
#include <optional>

namespace sysconf {
  constexpr int size = 0x4000;
  constexpr auto path = "/shared2/sys/SYSCONF";

  struct Display {
    std::optional<bool> widescreen;
    std::optional<bool> progressive_scan;
    std::optional<bool> pal60;
  };

  struct Settings {
    std::optional<bool> setup_complete;
    std::optional<QString> nickname;
    std::optional<wiiform::Language> language;
    std::optional<std::uint8_t> country;
    std::optional<bool> sensor_bar_above;
  };

  auto read_display(const QByteArray& bytes) -> Display;
  auto read_settings(const QByteArray& bytes) -> Settings;
  auto create(const wiiform::SystemSettings& settings, const QString& menu_region) -> QByteArray;
} // namespace sysconf
