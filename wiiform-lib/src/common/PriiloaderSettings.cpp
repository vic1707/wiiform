// SPDX-License-Identifier: GPL-2.0-only
#include <PriiloaderSettings.hpp>

#include <QtEndian>

#include <array>
#include <stdexcept>

namespace {
  // Priiloader tag 0.10.0 (2b70b36): Settings in settings.h and version.h.
  constexpr int settings_size = 32;
  constexpr quint32 merged_version_010 = 0x000a0000;

  struct EnumSetting {
    const char* name;
    int offset;
    const char* const* choices;
    std::size_t choice_count;
  };

  constexpr std::array autoboot_choices{
      "disabled", "homebrew_channel", "bootmii_ios", "system_menu", "file", "error"};
  constexpr std::array return_choices{"system_menu", "priiloader", "autoboot", "file"};
  constexpr std::array shutdown_choices{"none", "priiloader", "autoboot"};
  constexpr std::array mount_choices{"auto", "sd", "usb"};
  constexpr std::array enum_settings{
      EnumSetting{"autoboot", 0, autoboot_choices.data(), autoboot_choices.size()},
      EnumSetting{"return_to", 8, return_choices.data(), return_choices.size()},
      EnumSetting{"shutdown_to", 9, shutdown_choices.data(), shutdown_choices.size()},
      EnumSetting{"preferred_mount_point", 27, mount_choices.data(), mount_choices.size()},
  };

  struct BooleanSetting {
    const char* name;
    int offset;
    bool writable = true;
  };

  constexpr std::array boolean_settings{
      BooleanSetting{"stop_disc", 10},
      BooleanSetting{"lid_slot_on_error", 11},
      BooleanSetting{"ignore_shutdown_mode", 12},
      BooleanSetting{"use_system_menu_ios", 21, false},
      BooleanSetting{"black_background", 22},
      BooleanSetting{"dump_gecko_text", 23},
      BooleanSetting{"passcheck_priiloader", 24, false},
      BooleanSetting{"passcheck_menu", 25, false},
      BooleanSetting{"show_rc_updates", 26},
  };

  auto read_u32(const QByteArray& bytes, int offset) -> quint32 {
    return qFromBigEndian<quint32>(reinterpret_cast<const uchar*>(bytes.constData() + offset));
  }

  void write_u32(QByteArray& bytes, int offset, quint32 value) {
    qToBigEndian<quint32>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto byte(const QByteArray& bytes, int offset) -> int {
    return static_cast<uchar>(bytes.at(offset));
  }

  auto enum_value(const EnumSetting& setting, const QString& requested) -> int {
    for (std::size_t i = 0; i < setting.choice_count; ++i)
      if (requested == QLatin1String(setting.choices[i])) return static_cast<int>(i);
    throw std::runtime_error(std::string("Unsupported Priiloader setting value for ") + setting.name);
  }

  auto setting_value(const QByteArray& bytes, const EnumSetting& setting) -> QString {
    return QLatin1String(setting.choices[static_cast<std::size_t>(byte(bytes, setting.offset))]);
  }

  auto boolean_value(const QString& value) -> bool {
    if (value == "on") return true;
    if (value == "off") return false;
    throw std::runtime_error("Priiloader boolean settings must be on or off");
  }
}

namespace priiloader {
  auto read_settings(const QByteArray& bytes) -> Settings {
    if (bytes.size() != settings_size)
      throw std::runtime_error("Cannot write settings in an unsupported Priiloader format");

    const auto version = read_u32(bytes, 4);
    const auto release_candidate = read_u32(bytes, 16);
    if (version != merged_version_010 || release_candidate != 0)
      throw std::runtime_error("Cannot write settings in an unsupported Priiloader format");

    for (const auto& setting : boolean_settings)
      if (byte(bytes, setting.offset) > 1)
        throw std::runtime_error("Cannot write malformed Priiloader settings");

    for (const auto& setting : enum_settings)
      if (static_cast<std::size_t>(byte(bytes, setting.offset)) >= setting.choice_count)
        throw std::runtime_error("Cannot write malformed Priiloader settings");

    return {setting_value(bytes, enum_settings[0]), setting_value(bytes, enum_settings[1])};
  }

  auto default_settings() -> QByteArray {
    QByteArray bytes(settings_size, '\0');
    bytes[0] = char(3);
    write_u32(bytes, 4, merged_version_010);
    bytes[21] = char(1);
    bytes[22] = char(1);
    return bytes;
  }

  auto patch_settings(QByteArray bytes, const QMap<QString, QString>& overrides) -> QByteArray {
    read_settings(bytes);

    QMap<QString, const EnumSetting*> enum_by_name;
    for (const auto& setting : enum_settings) enum_by_name.insert(QLatin1String(setting.name), &setting);
    QMap<QString, int> boolean_by_name;
    for (const auto& setting : boolean_settings)
      if (setting.writable) boolean_by_name.insert(QLatin1String(setting.name), setting.offset);

    for (auto it = overrides.cbegin(); it != overrides.cend(); ++it) {
      const auto enum_setting = enum_by_name.constFind(it.key());
      if (enum_setting != enum_by_name.cend()) {
        if (it.key() == "autoboot" && it.value() == "error")
          throw std::runtime_error("The Priiloader autoboot error state cannot be selected");
        bytes[enum_setting.value()->offset] = static_cast<char>(enum_value(*enum_setting.value(), it.value()));
        continue;
      }
      const auto boolean_setting = boolean_by_name.constFind(it.key());
      if (boolean_setting != boolean_by_name.cend()) {
        bytes[boolean_setting.value()] = boolean_value(it.value()) ? char(1) : char(0);
        continue;
      }
      throw std::runtime_error(std::string("Unsupported Priiloader setting name: ") + it.key().toStdString());
    }
    return bytes;
  }
} // namespace priiloader
