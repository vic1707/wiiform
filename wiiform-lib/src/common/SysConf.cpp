// SPDX-License-Identifier: GPL-2.0-only
// Format references are Dolphin's SysConf.cpp, libogc's conf.c and WiiBrew's
// SYSCONF name lookup table, verified against original Wii captures.
#include <SysConf.hpp>

#include <QtEndian>
#include <algorithm>
#include <array>
#include <stdexcept>
#include <vector>

namespace {
  constexpr int header_size = 6;
  constexpr int footer_size = 4;
  constexpr int footer_offset = wii_sysconf::size - footer_size;
  constexpr int lookup_start = 0x3fae;

  struct DisplayEntry {
    const char* name;
    int lookup_offset;
  };

  constexpr std::array display_entries{
      DisplayEntry{"IPL.AR", 0x3ff8},
      DisplayEntry{"IPL.PGS", 0x3fde},
      DisplayEntry{"IPL.E60", 0x3fee},
  };

  enum class Type : quint8 {
    BigArray = 1,
    SmallArray = 2,
    Byte = 3,
    Long = 5,
    ByteBool = 7
  };

  struct EntryToWrite {
    QByteArray name;
    Type type;
    QByteArray value;
  };

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void write_u16(QByteArray& bytes, int offset, int value) {
    qToBigEndian<quint16>(static_cast<quint16>(value), reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto table_slot(int index) -> int {
    return header_size + index * 2;
  }

  auto encode_entry(const EntryToWrite& entry) -> QByteArray {
    QByteArray result;
    result.append(char((static_cast<int>(entry.type) << 5) | (entry.name.size() - 1)));
    result.append(entry.name);
    if (entry.type == Type::BigArray) {
      require(!entry.value.isEmpty() && entry.value.size() <= 0x10000,
              "SYSCONF big-array size is invalid");
      const auto length = entry.value.size() - 1;
      result.append(char((length >> 8) & 0xff));
      result.append(char(length & 0xff));
    } else if (entry.type == Type::SmallArray) {
      require(!entry.value.isEmpty() && entry.value.size() <= 0x100,
              "SYSCONF small-array size is invalid");
      result.append(char(entry.value.size() - 1));
    }
    result.append(entry.value);
    return result;
  }

  auto u32_bytes(quint32 value) -> QByteArray {
    QByteArray result(4, '\0');
    qToBigEndian<quint32>(value, reinterpret_cast<uchar*>(result.data()));
    return result;
  }

  auto nickname_bytes(const QString& nickname) -> QByteArray {
    require(!nickname.isEmpty() && nickname.size() <= 10 && !nickname.trimmed().isEmpty(),
            "Nickname must contain 1 to 10 nonblank BMP characters");
    QByteArray result(0x16, '\0');
    for (int index = 0; index < nickname.size(); ++index) {
      const auto character = nickname.at(index);
      require(character.isPrint() && !character.isSurrogate(),
              "Nickname contains an unsupported control or non-BMP character");
      qToBigEndian<quint16>(character.unicode(),
                            reinterpret_cast<uchar*>(result.data() + index * 2));
    }
    qToBigEndian<quint16>(static_cast<quint16>(nickname.size()),
                          reinterpret_cast<uchar*>(result.data() + 20));
    return result;
  }

  auto generated_entries(const wiiform::SystemSettings& settings,
                         const QString& menu_region) -> std::vector<EntryToWrite> {
    require(menu_region == "EUR" || menu_region == "USA" || menu_region == "JPN",
            "Unsupported menu region for SYSCONF generation");
    const auto country = settings.country;
    const bool country_allowed = menu_region == "EUR"
        ? (country == 65 || country == 66 || country == 67 || country == 74 || country == 76 ||
           country == 77 || country == 78 || country == 79 || country == 82 || country == 83 ||
           country == 88 || (country >= 94 && country <= 99) || country == 105 || country == 107 ||
           country == 108 || country == 110)
        : menu_region == "USA" ? (country >= 8 && country <= 52) : country == 1;
    require(country_allowed, "Country code does not match the requested menu region");

    using wiiform::Language;
    const auto language = settings.language;
    const auto language_index = static_cast<std::uint8_t>(language);
    require(language_index <= static_cast<std::uint8_t>(Language::nl), "Unsupported Wii language code");
    if (menu_region == "JPN") require(language == Language::ja, "Japanese menu requires Japanese language");
    if (menu_region == "USA") require(language == Language::en || language == Language::fr || language == Language::es,
                                       "Language is not supported by the USA menu");
    if (menu_region == "EUR") require(language != Language::ja, "Language is not supported by the EUR menu");
    require(settings.nickname.size() <= 30, "Nickname exceeds the supported UTF-8 size");
    const auto nickname = QString::fromStdString(settings.nickname);
    require(nickname.toStdString() == settings.nickname, "Nickname must be UTF-8");

    QByteArray country_data(0x1008, '\0');
    country_data[0] = char(country);
    QByteArray parental_controls(0x4a, '\0');
    parental_controls[1] = char(0x04);
    parental_controls[2] = char(0x14);
    const QByteArray idle("\0\1", 2);

    return {
      {"BT.DINF", Type::BigArray, QByteArray(0x461, '\0')},
      {"BT.CDIF", Type::BigArray, QByteArray(0x205, '\0')},
      {"BT.SENS", Type::Long, u32_bytes(3)},
      {"BT.BAR", Type::Byte, QByteArray(1, settings.sensor_bar_above ? char(1) : char(0))},
      {"BT.SPKV", Type::Byte, QByteArray(1, char(0x58))},
      {"BT.MOT", Type::Byte, QByteArray(1, char(1))},
      {"IPL.NIK", Type::SmallArray, nickname_bytes(nickname)},
      {"IPL.LNG", Type::Byte, QByteArray(1, char(language_index))},
      {"IPL.SADR", Type::BigArray, country_data},
      {"IPL.PC", Type::SmallArray, parental_controls},
      {"IPL.CB", Type::Long, u32_bytes(0)},
      {"IPL.AR", Type::Byte, QByteArray(1, settings.widescreen ? char(1) : char(0))},
      {"IPL.SSV", Type::Byte, QByteArray(1, char(1))},
      {"IPL.CD", Type::ByteBool, QByteArray(1, char(1))},
      {"IPL.CD2", Type::ByteBool, QByteArray(1, char(0))},
      {"IPL.EULA", Type::ByteBool, QByteArray(1, char(0))},
      {"IPL.UPT", Type::Byte, QByteArray(1, char(2))},
      {"IPL.PGS", Type::Byte, QByteArray(1, settings.progressive ? char(1) : char(0))},
      {"IPL.E60", Type::Byte, QByteArray(1, settings.pal60 ? char(1) : char(0))},
      {"IPL.DH", Type::Byte, QByteArray(1, char(0))},
      {"IPL.INC", Type::Long, u32_bytes(8)},
      {"IPL.FRC", Type::Long, u32_bytes(40)},
      {"IPL.IDL", Type::SmallArray, idle},
      {"NET.WCFG", Type::Long, u32_bytes(1)},
      {"NET.CTPC", Type::Long, u32_bytes(0)},
      {"WWW.RST", Type::Byte, QByteArray(1, char(0))},
      {"MPLS.MOVIE", Type::ByteBool, QByteArray(1, char(1))},
    };
  }

  auto encode_new_sysconf(const std::vector<EntryToWrite>& entries) -> QByteArray {
    QByteArray bytes(wii_sysconf::size, '\0');
    bytes.replace(0, 4, "SCv0");
    write_u16(bytes, 4, static_cast<int>(entries.size()));
    int offset = header_size + (static_cast<int>(entries.size()) + 1) * 2;
    for (int index = 0; index < static_cast<int>(entries.size()); ++index) {
      write_u16(bytes, table_slot(index), offset);
      const auto encoded = encode_entry(entries[static_cast<std::size_t>(index)]);
      require(offset + encoded.size() <= lookup_start, "Generated SYSCONF entries exceed their data area");
      bytes.replace(offset, encoded.size(), encoded);
      offset += encoded.size();
    }
    write_u16(bytes, table_slot(static_cast<int>(entries.size())), offset);
    for (const auto& display_entry : display_entries) {
      const auto found = std::find_if(entries.begin(), entries.end(), [&](const auto& entry) {
        return entry.name == display_entry.name;
      });
      require(found != entries.end(), "Generated SYSCONF is missing a display field");
      write_u16(bytes, display_entry.lookup_offset,
                table_slot(static_cast<int>(found - entries.begin())));
    }
    bytes.replace(footer_offset, footer_size, "SCed");
    return bytes;
  }

} // namespace

namespace wii_sysconf {
  auto create(const wiiform::SystemSettings& requested, const QString& menu_region) -> QByteArray {
    return encode_new_sysconf(generated_entries(requested, menu_region));
  }

} // namespace wii_sysconf
