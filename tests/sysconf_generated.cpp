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

  auto byte_value(quint8 value) -> QByteArray {
    return QByteArray(1, static_cast<char>(value));
  }

  void check_nickname(const QByteArray& payload, const QString& expected) {
    check(payload.size() == 0x16, "Generated nickname record has the wrong size");
    for (int index = 0; index < 10; ++index) {
      const auto code = read_u16(payload, index * 2);
      const auto expected_code = index < expected.size()
          ? expected.at(index).unicode()
          : quint16(0);
      check(code == expected_code, "Generated nickname UTF-16 bytes are incorrect");
    }
    check(read_u16(payload, 20) == expected.size(),
          "Generated nickname character count is incorrect");
  }

  auto entry_payload(const QByteArray& bytes, std::string_view wanted_name) -> QByteArray {
    const int count = read_u16(bytes, 4);
    for (int index = 0; index < count; ++index) {
      const int start = read_u16(bytes, 6 + index * 2);
      const int next = read_u16(bytes, 6 + (index + 1) * 2);
      check(start >= 6 + (count + 1) * 2 && start < next && next <= bytes.size() - 4,
            "Invalid SYSCONF entry offsets");
      const auto descriptor = static_cast<std::uint8_t>(bytes.at(start));
      const int name_size = (descriptor & 0x1f) + 1;
      check(start + 1 + name_size < next, "SYSCONF entry name exceeds its bounds");
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
    check(entry_payload(bytes, "IPL.CD") == byte_value(1),
          "Generated SYSCONF must mark first-time setup complete");
    check_nickname(entry_payload(bytes, "IPL.NIK"), QStringLiteral("Setup"));
    check(entry_payload(bytes, "IPL.LNG") == byte_value(static_cast<quint8>(language)),
          "Generated language value is incorrect");
    const auto country_payload = entry_payload(bytes, "IPL.SADR");
    check(country_payload.size() == 0x1008, "Generated country payload has the wrong size");
    check(static_cast<std::uint8_t>(country_payload.at(0)) == country,
          "IPL.SADR does not begin with the country code");
    check(entry_payload(bytes, "BT.BAR") == byte_value(0),
          "Generated sensor-bar position is incorrect");

    const wiiform::SystemSettings alternate{"Alternate", language, country, true, false, false, false};
    const auto alternate_bytes = sysconf::create(alternate, menu);
    check(entry_payload(alternate_bytes, "IPL.CD") == byte_value(1),
          "Alternate SYSCONF must mark first-time setup complete");
    check_nickname(entry_payload(alternate_bytes, "IPL.NIK"), QStringLiteral("Alternate"));
    check(entry_payload(alternate_bytes, "IPL.LNG") == byte_value(static_cast<quint8>(language)),
          "Alternate generated language value is incorrect");
    check(entry_payload(alternate_bytes, "BT.BAR") == byte_value(1),
          "Alternate generated sensor-bar position is incorrect");
    check(entry_payload(alternate_bytes, "IPL.AR") == byte_value(0) &&
              entry_payload(alternate_bytes, "IPL.PGS") == byte_value(0) &&
              entry_payload(alternate_bytes, "IPL.E60") == byte_value(0),
          "Alternate generated display values are incorrect");

    check(country_payload.mid(1, 3) == QByteArray(3, '\0'),
          "Generated country subdivision bytes must retain their zero defaults");

    check(entry_payload(bytes, "BT.DINF") == QByteArray(0x461, '\0'),
          "Generated SYSCONF must start with no paired Wii Remotes");
    check(entry_payload(bytes, "BT.CDIF") == QByteArray(0x205, '\0'),
          "Generated SYSCONF must start with no guest Wii Remotes");
    check(entry_payload(bytes, "IPL.PC").size() == 0x4a &&
              entry_payload(bytes, "IPL.IDL").size() == 2,
          "Generated SYSCONF array lengths must match real Wii records");
    check(entry_payload(bytes, "IPL.AR") == byte_value(1) &&
              entry_payload(bytes, "IPL.PGS") == byte_value(1) &&
              entry_payload(bytes, "IPL.E60") == byte_value(1),
          "Generated display values are incorrect");
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
