// SPDX-License-Identifier: GPL-2.0-only
#include <SysConf.hpp>

#include <QCoreApplication>
#include <QtEndian>

#include <cstdint>
#include <iostream>
#include <stdexcept>
#include <string>
#include <string_view>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto read_u16(const QByteArray& bytes, int offset) -> std::uint16_t {
    return qFromBigEndian<std::uint16_t>(
        reinterpret_cast<const uchar*>(bytes.constData() + offset));
  }

  auto entry_payload(const QByteArray& bytes, std::string_view wanted_name) -> QByteArray {
    const int count = read_u16(bytes, 4);
    for (int index = 0; index < count; ++index) {
      const int start = read_u16(bytes, 6 + index * 2);
      const int next = read_u16(bytes, 6 + (index + 1) * 2);
      const auto descriptor = static_cast<std::uint8_t>(bytes.at(start));
      const int name_size = (descriptor & 0x1f) + 1;
      const auto name = std::string_view(bytes.constData() + start + 1, name_size);
      if (name != wanted_name) continue;

      const int data = start + 1 + name_size;
      switch (descriptor >> 5) {
        case 1: {
          const int size = int(read_u16(bytes, data)) + 1;
          check(data + 2 + size <= next, "SYSCONF big-array entry exceeds its bounds");
          return bytes.mid(data + 2, size);
        }
        case 2: {
          const int size = static_cast<std::uint8_t>(bytes.at(data)) + 1;
          check(data + 1 + size <= next, "SYSCONF small-array entry exceeds its bounds");
          return bytes.mid(data + 1, size);
        }
        default:
          check(data + 1 <= next, "SYSCONF scalar entry exceeds its bounds");
          return bytes.mid(data, next - data);
      }
    }
    throw std::runtime_error("Generated SYSCONF is missing an expected entry");
  }

  void check_region(const QString& menu, std::uint8_t country) {
    using wiiform::Language;
    const auto language = menu == "JPN" ? Language::ja : Language::en;
    const wiiform::SystemSettings requested{"Setup", language, country, false, true, true, true};

    const auto bytes = sysconf::create(requested, menu);
    check(bytes.size() == sysconf::size, "Generated SYSCONF has the wrong size");
    check(bytes.startsWith("SCv0") && bytes.mid(bytes.size() - 4) == "SCed",
          "Generated SYSCONF has invalid sentinels");
    const auto settings = sysconf::read_settings(bytes);
    check(settings.setup_complete == true, "Generated SYSCONF did not skip first-time setup");
    check(settings.nickname == QStringLiteral("Setup"), "Generated nickname did not round-trip");
    check(settings.language == language,
          "Generated language did not round-trip");
    check(settings.country == country, "Generated country did not round-trip");
    check(settings.sensor_bar_above == false, "Generated sensor-bar position did not round-trip");

    const wiiform::SystemSettings alternate{"Alternate", language, country, true, false, false, false};
    const auto alternate_bytes = sysconf::create(alternate, menu);
    const auto alternate_settings = sysconf::read_settings(alternate_bytes);
    const auto alternate_display = sysconf::read_display(alternate_bytes);
    check(alternate_settings.setup_complete == true &&
              alternate_settings.nickname == QStringLiteral("Alternate") &&
              alternate_settings.language == language && alternate_settings.country == country &&
              alternate_settings.sensor_bar_above == true,
          "Alternate generated setup fields did not round-trip");
    check(alternate_display.widescreen == false && alternate_display.progressive_scan == false &&
              alternate_display.pal60 == false,
          "Alternate generated display fields did not round-trip");

    auto pending_setup = requested;
    pending_setup.setup_complete = false;
    const auto pending_bytes = sysconf::create(pending_setup, menu);
    check(entry_payload(pending_bytes, "IPL.CD") == QByteArray(1, '\0') &&
              sysconf::read_settings(pending_bytes).setup_complete == false,
          "Display-only settings must leave first-time setup pending");
    check(sysconf::read_display(pending_bytes).widescreen == true,
          "Pending first-time setup must retain the requested display settings");

    const auto country_payload = entry_payload(bytes, "IPL.SADR");
    check(country_payload.size() == 0x1008, "Generated country payload has the wrong size");
    check(static_cast<std::uint8_t>(country_payload.at(0)) == country,
          "IPL.SADR does not begin with the country code");
    check(country_payload.mid(1, 3) == QByteArray(3, '\0'),
          "Generated country subdivision bytes must retain their zero defaults");

    check(entry_payload(bytes, "BT.DINF") == QByteArray(0x461, '\0'),
          "Generated SYSCONF must start with no paired Wii Remotes");
    check(entry_payload(bytes, "BT.CDIF") == QByteArray(0x205, '\0'),
          "Generated SYSCONF must start with no guest Wii Remotes");
    check(entry_payload(bytes, "IPL.PC").size() == 0x4a &&
              entry_payload(bytes, "IPL.IDL").size() == 2,
          "Generated SYSCONF array lengths must match real Wii records");
    const auto bar_payload = entry_payload(bytes, "BT.BAR");
    check(bar_payload.size() == 1 && bar_payload.at(0) == 0,
          "BT.BAR did not encode the below position");
    const auto display = sysconf::read_display(bytes);
    check(display.widescreen == true && display.progressive_scan == true && display.pal60 == true,
          "Generated display flags did not round-trip");
  }
} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    check_region(QStringLiteral("EUR"), 110);
    check_region(QStringLiteral("USA"), 49);
    check_region(QStringLiteral("JPN"), 1);

    const wiiform::SystemSettings invalid{"Wii", wiiform::Language::en, 1, true, false, false, false};
    try {
      (void)sysconf::create(invalid, QStringLiteral("EUR"));
      throw std::runtime_error("SYSCONF accepted a country incompatible with its menu");
    } catch (const std::runtime_error& error) {
      check(std::string_view(error.what()) == "Country code does not match the requested menu region",
            "SYSCONF country validation returned an unexpected error");
    }

    std::cout << "Generated SYSCONF layout and setup values passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
