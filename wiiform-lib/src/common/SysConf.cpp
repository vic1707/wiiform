// SPDX-License-Identifier: GPL-2.0-only
// Format references are Dolphin's SysConf.cpp, libogc's conf.c and WiiBrew's
// SYSCONF name lookup table, verified against original Wii captures.
#include <SysConf.hpp>

#include <QJsonObject>
#include <QMap>
#include <QtEndian>
#include <QSet>
#include <algorithm>
#include <array>
#include <stdexcept>
#include <utility>

namespace {
  constexpr int header_size = 6;
  constexpr int footer_size = 4;
  constexpr int footer_offset = sysconf::size - footer_size;
  constexpr int lookup_start = 0x3fae;

  struct DisplayEntry {
    const char* name;
    int lookup_offset;
    std::optional<bool> sysconf::DisplayOverrides::* option;
  };

  constexpr std::array display_entries{
      DisplayEntry{"IPL.AR", 0x3ff8, &sysconf::DisplayOverrides::widescreen},
      DisplayEntry{"IPL.PGS", 0x3fde, &sysconf::DisplayOverrides::progressive_scan},
      DisplayEntry{"IPL.E60", 0x3fee, &sysconf::DisplayOverrides::pal60},
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

  struct Addition {
    const DisplayEntry* entry;
    bool value;
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
      if (name == "IPL.CD" || name == "IPL.NIK" || name == "IPL.LNG") {
        Record record{type, -1, 0, value_offset};
        if (type == static_cast<quint8>(Type::SmallArray)) {
          record.payload_offset = data_offset + 1;
          record.payload_size = int(static_cast<quint8>(bytes.at(data_offset))) + 1;
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

  auto language_names() -> const std::array<QString, 7>& {
    static const std::array<QString, 7> names{QStringLiteral("ja"), QStringLiteral("en"), QStringLiteral("de"),
                                               QStringLiteral("fr"), QStringLiteral("es"), QStringLiteral("it"),
                                               QStringLiteral("nl")};
    return names;
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

  auto patch_settings_impl(QByteArray bytes, const sysconf::SettingsOverrides& overrides) -> QByteArray {
    const auto parsed = parse(bytes);
    const auto setup = parsed.records.value("IPL.CD");
    if (overrides.require_setup) {
      require(setup.value_offset >= 0 &&
                  (setup.type == static_cast<quint8>(Type::Byte) ||
                   setup.type == static_cast<quint8>(Type::ByteBool)),
              "SYSCONF has no supported IPL.CD setup flag");
      const auto value = static_cast<quint8>(bytes.at(setup.value_offset));
      require(value == 1, "SYSCONF indicates Wii first-time setup is incomplete");
    }

    if (overrides.nickname) {
      const auto found = parsed.records.constFind("IPL.NIK");
      require(found != parsed.records.cend(), "SYSCONF has no IPL.NIK nickname entry");
      require(found->type == static_cast<quint8>(Type::SmallArray) && found->payload_size == 0x16,
              "Unsupported IPL.NIK layout");
      const auto& name = *overrides.nickname;
      require(!name.isEmpty() && name.size() <= 10 && !name.trimmed().isEmpty(),
              "Nickname must contain 1 to 10 nonblank BMP characters");
      QByteArray utf16(0x16, '\0');
      for (int i = 0; i < name.size(); ++i) {
        const auto ch = name.at(i);
        require(ch.isPrint() && !ch.isSurrogate(),
                "Nickname contains an unsupported control or non-BMP character");
        qToBigEndian<quint16>(ch.unicode(), reinterpret_cast<uchar*>(utf16.data() + 2 * i));
      }
      qToBigEndian<quint16>(static_cast<quint16>(name.size()),
                            reinterpret_cast<uchar*>(utf16.data() + 20));
      bytes.replace(found->payload_offset, found->payload_size, utf16);
    }

    if (overrides.language) {
      const auto found = parsed.records.constFind("IPL.LNG");
      require(found != parsed.records.cend() && found->type == static_cast<quint8>(Type::Byte) &&
                  found->value_offset >= 0,
              "SYSCONF has no supported IPL.LNG language entry");
      const auto& names = language_names();
      const auto found_language = std::find(names.cbegin(), names.cend(), *overrides.language);
      require(found_language != names.cend(), "Unsupported Wii language code");
      bytes[found->value_offset] = char(found_language - names.cbegin());
    }
    return sysconf::patch_display(bytes, overrides.display);
  }

  auto append_display_fields(const QByteArray& bytes, const Parsed& parsed,
                             const QList<Addition>& additions) -> QByteArray {
    const int count = static_cast<int>(parsed.offsets.size()) - 1;
    const int end = parsed.offsets.last();
    require(end <= lookup_start, "SYSCONF entries overlap the name lookup table");
    for (int slot = lookup_start; slot < footer_offset; slot += 2) {
      const int address = read_u16(bytes, slot);
      require(!address || (address >= header_size && address < table_slot(count) && address % 2 == 0),
              "Invalid SYSCONF name lookup table");
    }
    bool known_layout = false;
    for (std::size_t i = 0; i < display_entries.size(); ++i) {
      const int address = read_u16(bytes, display_entries[i].lookup_offset);
      require(address == parsed.fields[i].table_offset, "SYSCONF display lookup does not match its entry");
      known_layout |= address != 0;
    }
    require(known_layout, "Adding display fields requires a recognized Wii SYSCONF lookup table");

    const int extra = static_cast<int>(additions.size());
    const int shift = 2 * extra;
    const int old_table_end = table_slot(count + 1);
    QByteArray records;
    for (const auto& addition : additions) {
      const QByteArray name(addition.entry->name);
      records += char((static_cast<int>(Type::Byte) << 5) | (name.size() - 1));
      records += name;
      records += char(addition.value);
    }
    require(end + shift + records.size() <= lookup_start, "Insufficient SYSCONF space for display fields");
    const auto padding = bytes.mid(end, lookup_start - end);
    require(std::all_of(padding.cbegin(), padding.cend(), [](char byte) { return byte == 0; }),
            "SYSCONF contains unrecognized data after its entries");

    // Keep every existing table cell at the same address so its lookup entry
    // still points to it. Only the underlying records need to move.
    auto result = bytes;
    result.replace(old_table_end + shift, end - old_table_end, bytes.mid(old_table_end, end - old_table_end));
    result.replace(end + shift, records.size(), records);
    write_u16(result, 4, count + extra);
    for (int i = 0; i < count; ++i)
      write_u16(result, table_slot(i), parsed.offsets[i] + shift);
    int offset = end + shift;
    for (int i = 0; i < extra; ++i) {
      const auto& entry = *additions[i].entry;
      const int slot = table_slot(count + i);
      write_u16(result, slot, offset);
      write_u16(result, entry.lookup_offset, slot);
      offset += static_cast<int>(qstrlen(entry.name)) + 2;
    }
    write_u16(result, table_slot(count + extra), offset);
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
      if (index < language_names().size()) result.language = language_names()[index];
    }
    return result;
  }

  auto patch_settings(QByteArray bytes, const SettingsOverrides& overrides) -> QByteArray {
    return patch_settings_impl(std::move(bytes), overrides);
  }

  auto settings_report(const Settings& settings) -> QJsonObject {
    auto optional = [](const auto& value) -> QJsonValue {
      return value ? QJsonValue(*value) : QJsonValue(QJsonValue::Null);
    };
    return {{"setup_complete", optional(settings.setup_complete)},
            {"nickname", optional(settings.nickname)}, {"language", optional(settings.language)}};
  }

  auto patch_display(QByteArray bytes, const DisplayOverrides& overrides) -> QByteArray {
    const auto parsed = ::parse(bytes);
    QList<Addition> additions;
    for (std::size_t i = 0; i < display_entries.size(); ++i) {
      const auto& entry = display_entries[i];
      const auto value = overrides.*entry.option;
      if (!value) continue;
      const auto offset = parsed.fields[i].value_offset;
      if (offset >= 0) bytes[offset] = char(*value);
      else additions.append({&entry, *value});
    }
    return additions.isEmpty() ? bytes : append_display_fields(bytes, parsed, additions);
  }

  auto display_report(const Display& display) -> QJsonObject {
    auto boolean = [](const std::optional<bool>& value) -> QJsonValue {
      return value ? QJsonValue(*value) : QJsonValue(QJsonValue::Null);
    };
    QJsonValue aspect(QJsonValue::Null);
    if (display.widescreen) aspect = *display.widescreen ? "16:9" : "4:3";
    return {{"aspect_ratio", aspect}, {"progressive_scan", boolean(display.progressive_scan)},
            {"pal60", boolean(display.pal60)}};
  }

} // namespace sysconf
