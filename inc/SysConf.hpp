// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QString>
#include <optional>

namespace sysconf {
  constexpr int size = 0x4000;
  constexpr auto path = "/shared2/sys/SYSCONF";
  constexpr int key_header_bytes = 0x100;

  struct Source {
    QByteArray bytes;
    QByteArray keys_data_sha256;
    QByteArray settings_sha256;
  };

  struct Display {
    std::optional<bool> widescreen;
    std::optional<bool> progressive_scan;
    std::optional<bool> pal60;
  };

  struct DisplayOverrides {
    std::optional<bool> widescreen;
    std::optional<bool> progressive_scan;
    std::optional<bool> pal60;
  };

  struct SettingsOverrides {
    bool require_setup = false;
    std::optional<QString> nickname;
    std::optional<QString> language;
    DisplayOverrides display;
  };

  struct Settings {
    std::optional<bool> setup_complete;
    std::optional<QString> nickname;
    std::optional<QString> language;
  };

  // Missing display fields can be added to a recognized Wii lookup layout.
  // Invalid or unsupported data raises a fixed, payload-free diagnostic.
  auto read_display(const QByteArray& bytes) -> Display;
  auto read_settings(const QByteArray& bytes) -> Settings;
  auto patch_settings(QByteArray bytes, const SettingsOverrides& overrides) -> QByteArray;
  auto settings_report(const Settings& settings) -> QJsonObject;
  auto patch_display(QByteArray bytes, const DisplayOverrides& overrides) -> QByteArray;
  auto display_report(const Display& display) -> QJsonObject;
} // namespace sysconf
