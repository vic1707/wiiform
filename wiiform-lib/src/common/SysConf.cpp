// SPDX-License-Identifier: GPL-2.0-only
// Format references are Dolphin's SysConf.cpp, libogc's conf.c and WiiBrew's
// SYSCONF name lookup table, verified against original Wii captures.
#include <SysConf.hpp>

#include <QMap>
#include <QtEndian>
#include <QSet>
#include <algorithm>
#include <array>
#include <stdexcept>
#include <vector>

namespace {
  constexpr int header_size = 6;
  constexpr int footer_size = 4;
  constexpr int footer_offset = sysconf::size - footer_size;
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
    Short = 4,
    Long = 5,
    LongLong = 6,
    ByteBool = 7
  };

  struct Field {
    int value_offset = -1;
    int table_offset = 0;
    quint8 value = 0;
  };

  struct Record {
    quint8 type = 0;
    int payload_offset = -1;
    int payload_size = 0;
    int value_offset = -1;
  };

  struct Parsed {
    QList<int> offsets;
    std::array<Field, display_entries.size()> fields{};
    QMap<QByteArray, Record> records;
  };

  struct EntryToWrite {
    QByteArray name;
    Type type;
    QByteArray value;
  };

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto read_u16(const QByteArray& bytes, int offset) -> quint16 {
    return qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(bytes.constData() + offset));
  }

  void write_u16(QByteArray& bytes, int offset, int value) {
    qToBigEndian<quint16>(static_cast<quint16>(value), reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto table_slot(int index) -> int {
    return header_size + index * 2;
  }

  auto read_entry_value(const QByteArray& bytes, int offset, int limit, quint8 type) -> int {
    const auto remaining = limit - offset;
    require(remaining >= 0, "Invalid SYSCONF entry bounds");
    switch (static_cast<Type>(type)) {
      case Type::BigArray: {
        require(remaining >= 2, "Invalid SYSCONF entry bounds");
        const int length = int(read_u16(bytes, offset)) + 1;
        require(length <= remaining - 2, "Invalid SYSCONF entry bounds");
        return -1;
      }
      case Type::SmallArray: {
        require(remaining >= 1, "Invalid SYSCONF entry bounds");
        const int length = int(static_cast<quint8>(bytes.at(offset))) + 1;
        require(length <= remaining - 1, "Invalid SYSCONF entry bounds");
        return -1;
      }
      case Type::Byte:
      case Type::ByteBool:
        require(remaining >= 1, "Invalid SYSCONF entry bounds");
        return offset;
      case Type::Short:
        require(remaining >= 2, "Invalid SYSCONF entry bounds");
        return -1;
      case Type::Long:
        require(remaining >= 4, "Invalid SYSCONF entry bounds");
        return -1;
      case Type::LongLong:
        require(remaining >= 8, "Invalid SYSCONF entry bounds");
        return -1;
    }
    throw std::runtime_error("Unsupported SYSCONF entry type");
  }

  auto parse(const QByteArray& bytes) -> Parsed {
    require(bytes.size() == sysconf::size, "Invalid SYSCONF size");
    require(bytes.startsWith("SCv0"), "Invalid SYSCONF header");
    require(bytes.mid(footer_offset, footer_size) == "SCed", "Invalid SYSCONF footer");

    const int count = int(read_u16(bytes, 4));
    const qint64 table_end_wide = qint64(header_size) + (qint64(count) + 1) * 2;
    require(table_end_wide <= footer_offset, "Invalid SYSCONF offset table");
    const int table_end = static_cast<int>(table_end_wide);

    Parsed parsed;
    auto& offsets = parsed.offsets;
    offsets.reserve(count + 1);
    for (int i = 0; i <= count; ++i) {
      const int offset = read_u16(bytes, table_slot(i));
      require(offset >= table_end && offset <= footer_offset, "Invalid SYSCONF entry offset");
      if (!offsets.isEmpty())
        require(offset > offsets.last(), "Invalid SYSCONF entry order");
      offsets.append(offset);
    }

    QSet<QByteArray> names;
    for (int i = 0; i < count; ++i) {
      const int start = offsets.at(i);
      const int limit = offsets.at(i + 1);
      require(limit > start && limit - start >= 1, "Invalid SYSCONF entry bounds");

      const auto descriptor = static_cast<quint8>(bytes.at(start));
      const quint8 type = descriptor >> 5;
      const int name_length = (descriptor & 0x1f) + 1;
      require(name_length <= limit - start - 1, "Invalid SYSCONF entry name");
      const auto name = bytes.mid(start + 1, name_length);
      require(!names.contains(name), "Duplicate SYSCONF entry name");
      names.insert(name);

      const int data_offset = start + 1 + name_length;
      const int value_offset = read_entry_value(bytes, data_offset, limit, type);
      if (name == "IPL.CD" || name == "IPL.NIK" || name == "IPL.LNG" ||
          name == "IPL.SADR" || name == "BT.BAR") {
        Record record{type, -1, 0, value_offset};
        if (type == static_cast<quint8>(Type::SmallArray)) {
          record.payload_offset = data_offset + 1;
          record.payload_size = int(static_cast<quint8>(bytes.at(data_offset))) + 1;
        } else if (type == static_cast<quint8>(Type::BigArray)) {
          record.payload_offset = data_offset + 2;
          record.payload_size = int(read_u16(bytes, data_offset)) + 1;
        }
        parsed.records.insert(name, record);
      }
      const auto found = std::find_if(display_entries.begin(), display_entries.end(),
                                      [&](const auto& entry) { return name == entry.name; });
      if (found == display_entries.end()) continue;
      require(type == static_cast<quint8>(Type::Byte) || type == static_cast<quint8>(Type::ByteBool),
              "Invalid SYSCONF display field type");
      require(value_offset >= data_offset, "Invalid SYSCONF display field size");
      const auto value = static_cast<quint8>(bytes.at(value_offset));
      require(value <= 1, "Invalid SYSCONF display field value");
      parsed.fields[found - display_entries.begin()] = {value_offset, table_slot(i), value};
    }

    return parsed;
  }

  auto display_from(const Parsed& parsed) -> sysconf::Display {
    auto value = [](const Field& field) -> std::optional<bool> {
      if (field.value_offset < 0) return std::nullopt;
      return field.value != 0;
    };
    return {value(parsed.fields[0]), value(parsed.fields[1]), value(parsed.fields[2])};
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
      {"IPL.CD", Type::ByteBool, QByteArray(1, char(settings.setup_complete))},
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
    QByteArray bytes(sysconf::size, '\0');
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

  auto nickname_record(const QByteArray& bytes, const Record& record) -> QString {
    require(record.type == static_cast<quint8>(Type::SmallArray) && record.payload_size == 0x16,
            "Unsupported IPL.NIK layout");
    const auto* data = reinterpret_cast<const uchar*>(bytes.constData() + record.payload_offset);
    const auto count = qFromBigEndian<quint16>(data + 20);
    require(count <= 10, "Invalid IPL.NIK character count");
    QString result;
    result.reserve(count);
    for (int i = 0; i < 10; ++i) {
      const auto code = qFromBigEndian<quint16>(data + 2 * i);
      if (i < count) {
        require(QChar(code).isPrint() && !QChar(code).isSurrogate(),
                "Unsupported IPL.NIK character");
        result.append(QChar(code));
      } else {
        require(code == 0, "Invalid IPL.NIK padding");
      }
    }
    require(!result.trimmed().isEmpty(), "Empty IPL.NIK nickname");
    return result;
  }

} // namespace

namespace sysconf {
  auto read_display(const QByteArray& bytes) -> Display {
    const auto parsed = ::parse(bytes);
    return display_from(parsed);
  }

  auto read_settings(const QByteArray& bytes) -> Settings {
    const auto parsed = ::parse(bytes);
    Settings result;
    const auto setup = parsed.records.constFind("IPL.CD");
    if (setup != parsed.records.cend()) {
      if ((setup->type == static_cast<quint8>(Type::Byte) ||
           setup->type == static_cast<quint8>(Type::ByteBool)) && setup->value_offset >= 0) {
        const auto value = static_cast<quint8>(bytes.at(setup->value_offset));
        if (value <= 1) result.setup_complete = value == 1;
      }
    }
    const auto nickname = parsed.records.constFind("IPL.NIK");
    if (nickname != parsed.records.cend()) {
      try {
        result.nickname = nickname_record(bytes, *nickname);
      } catch (const std::exception&) {
        // Read-only inspection should still report display settings if an unrelated field is unusual.
      }
    }
    const auto language = parsed.records.constFind("IPL.LNG");
    if (language != parsed.records.cend() && language->type == static_cast<quint8>(Type::Byte) &&
        language->value_offset >= 0) {
      const auto index = static_cast<quint8>(bytes.at(language->value_offset));
      if (index <= static_cast<std::uint8_t>(wiiform::Language::nl))
        result.language = static_cast<wiiform::Language>(index);
    }
    const auto country = parsed.records.constFind("IPL.SADR");
    if (country != parsed.records.cend() &&
        country->type == static_cast<quint8>(Type::BigArray) && country->payload_size == 0x1008) {
      result.country = static_cast<std::uint8_t>(bytes.at(country->payload_offset));
    }
    const auto sensor_bar = parsed.records.constFind("BT.BAR");
    if (sensor_bar != parsed.records.cend() && sensor_bar->type == static_cast<quint8>(Type::Byte) &&
        sensor_bar->value_offset >= 0) {
      const auto value = static_cast<quint8>(bytes.at(sensor_bar->value_offset));
      if (value <= 1) result.sensor_bar_above = value == 1;
    }
    return result;
  }

  auto create(const wiiform::SystemSettings& requested, const QString& menu_region) -> QByteArray {
    return encode_new_sysconf(generated_entries(requested, menu_region));
  }

} // namespace sysconf
