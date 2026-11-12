// SPDX-License-Identifier: GPL-2.0-only
#include <SysConf.hpp>
#include <Display.hpp>
#include <Titles.hpp>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QtEndian>
#include <algorithm>
#include <functional>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
  struct Entry {
    QByteArray name;
    quint8 type;
    quint8 value;
    QByteArray payload{};
  };

  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void rejects(const std::function<void()>& operation, const char* label) {
    try {
      operation();
    } catch (const std::exception&) {
      return;
    }
    throw std::runtime_error(std::string("SYSCONF accepted ") + label);
  }

  auto put16(QByteArray& bytes, int offset, quint16 value) -> void {
    qToBigEndian<quint16>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto get16(const QByteArray& bytes, int offset) -> quint16 {
    return qFromBigEndian<quint16>(reinterpret_cast<const uchar*>(bytes.constData() + offset));
  }

  auto make_sysconf(const std::vector<Entry>& entries) -> QByteArray {
    QByteArray bytes(sysconf::size, '\xff');
    bytes.replace(0, 4, "SCv0");
    bytes.replace(0x3ffc, 4, "SCed");
    put16(bytes, 4, static_cast<quint16>(entries.size()));

    int cursor = 6 + static_cast<int>((entries.size() + 1) * 2);
    for (std::size_t index = 0; index < entries.size(); ++index) {
      const auto& entry = entries[index];
      check(!entry.name.isEmpty() && entry.name.size() <= 32, "Invalid synthetic entry name");
      put16(bytes, 6 + static_cast<int>(index * 2), static_cast<quint16>(cursor));
      bytes[cursor++] = static_cast<char>((entry.type << 5) | (entry.name.size() - 1));
      bytes.replace(cursor, entry.name.size(), entry.name);
      cursor += entry.name.size();
      if (entry.type == 2) {
        check(entry.payload.size() <= 0x100, "Invalid synthetic SmallArray");
        bytes[cursor++] = static_cast<char>(entry.payload.size() - 1);
        bytes.replace(cursor, entry.payload.size(), entry.payload);
        cursor += entry.payload.size();
      } else if (entry.type == 1) {
        check(entry.payload.size() <= 0x10000, "Invalid synthetic BigArray");
        put16(bytes, cursor, static_cast<quint16>(entry.payload.size() - 1));
        cursor += 2;
        bytes.replace(cursor, entry.payload.size(), entry.payload);
        cursor += entry.payload.size();
      } else {
        bytes[cursor++] = static_cast<char>(entry.value);
      }
    }
    put16(bytes, 6 + static_cast<int>(entries.size() * 2), static_cast<quint16>(cursor));
    return bytes;
  }

  auto target_entries(quint8 aspect = 0, quint8 progressive = 1, quint8 pal60 = 0)
      -> std::vector<Entry> {
    return {{"IPL.AR", 3, aspect}, {"IPL.PGS", 7, progressive}, {"IPL.E60", 7, pal60},
            {"OTHER", 3, 0x5a}};
  }

  auto nickname_payload(const QString& name) -> QByteArray {
    QByteArray result(22, '\0');
    for (int i = 0; i < name.size(); ++i)
      qToBigEndian<quint16>(name.at(i).unicode(), reinterpret_cast<uchar*>(result.data() + i * 2));
    qToBigEndian<quint16>(static_cast<quint16>(name.size()),
                          reinterpret_cast<uchar*>(result.data() + 20));
    return result;
  }

  auto configured_sysconf() -> QByteArray {
    return make_sysconf({{"IPL.CD", 7, 1}, {"IPL.NIK", 2, 0, nickname_payload("Parents")},
                         {"IPL.LNG", 3, 1}, {"IPL.AR", 3, 0},
                         {"IPL.PGS", 7, 1}, {"IPL.E60", 7, 0}, {"OTHER", 3, 0x5a}});
  }

  auto assert_only_value_changed(const QByteArray& before, const QByteArray& after,
                                 const std::vector<int>& allowed) -> void {
    check(before.size() == after.size(), "SYSCONF patch changed image size");
    for (int i = 0; i < before.size(); ++i) {
      if (std::find(allowed.begin(), allowed.end(), i) == allowed.end())
        check(before[i] == after[i], "SYSCONF patch changed an unrelated byte");
    }
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(file.open(QIODevice::ReadOnly) && hash.addData(&file), "Cannot hash SYSCONF test image");
    return hash.result();
  }

  void test_missing_fields() {
    auto original = make_sysconf({{"IPL.AR", 3, 0}, {"OTHER", 3, 0x5a}});
    const int original_end = get16(original, 10);
    std::fill(original.begin() + original_end, original.begin() + 0x3ffc, '\0');
    put16(original, 0x3ff8, 6);
    put16(original, 0x3fae, 8);
    sysconf::DisplayOverrides overrides;
    overrides.progressive_scan = false;
    overrides.pal60 = true;
    const auto expanded = sysconf::patch_display(original, overrides);
    const auto display = sysconf::read_display(expanded);
    check(display.widescreen == false && display.progressive_scan == false && display.pal60 == true,
          "Appended display values were not read back");
    check(expanded.size() == original.size() && get16(expanded, 4) == 4 &&
              get16(expanded, 0x3fde) == 10 && get16(expanded, 0x3fee) == 12 &&
              get16(expanded, 0x3ff8) == 6 && get16(expanded, 0x3fae) == 8,
          "Appending fields did not preserve or update name lookup cells");
    const int old_records = get16(original, 6);
    check(get16(expanded, 6) == old_records + 4 &&
              expanded.mid(old_records + 4, original_end - old_records) ==
                  original.mid(old_records, original_end - old_records),
          "Appending display fields changed existing entry records");
    check(get16(expanded, 14) == original_end + 4 + 18 && expanded.endsWith("SCed"),
          "Appending display fields did not preserve the record sentinel or footer");
    check(sysconf::patch_display(expanded, overrides) == expanded, "Repeated append was not a no-op");

    auto bad_lookup = original;
    put16(bad_lookup, 0x3fde, 8);
    rejects([&] { sysconf::patch_display(bad_lookup, overrides); }, "a conflicting display lookup");
    put16(bad_lookup, 0x3fae, 10);
    rejects([&] { sysconf::patch_display(bad_lookup, overrides); }, "a lookup into the past-end sentinel");
    auto nonzero_padding = original;
    nonzero_padding[original_end] = 'x';
    rejects([&] { sysconf::patch_display(nonzero_padding, overrides); }, "unknown trailing SYSCONF data");
    auto full = original;
    put16(full, 10, 0x3fae - 1);
    rejects([&] { sysconf::patch_display(full, overrides); }, "a SYSCONF with insufficient space");
  }

  void test_setup_settings() {
    const auto original = configured_sysconf();
    const auto settings = sysconf::read_settings(original);
    check(settings.setup_complete == true && settings.nickname == QString("Parents") &&
              settings.language == QString("en"),
          "Configured setup settings were not decoded");

    sysconf::SettingsOverrides overrides;
    overrides.require_setup = true;
    overrides.nickname = "Console 2";
    overrides.language = "fr";
    overrides.display.widescreen = true;
    const auto updated = sysconf::patch_settings(original, overrides);
    const auto after = sysconf::read_settings(updated);
    check(after.setup_complete == true && after.nickname == QString("Console 2") &&
              after.language == QString("fr") && sysconf::read_display(updated).widescreen == true,
          "Setup settings were not patched");

    const int count = get16(original, 4);
    int nickname_data = -1;
    int language_value = -1;
    int display_value = -1;
    for (int i = 0; i < count; ++i) {
      const int start = get16(original, 6 + i * 2);
      const int name_size = (static_cast<quint8>(original[start]) & 0x1f) + 1;
      const auto name = original.mid(start + 1, name_size);
      const int data = start + 1 + name_size;
      if (name == "IPL.NIK") nickname_data = data + 1;
      if (name == "IPL.LNG") language_value = data;
      if (name == "IPL.AR") display_value = data;
    }
    check(nickname_data >= 0 && language_value >= 0 && display_value >= 0,
          "Settings test fixture is incomplete");
    std::vector<int> allowed;
    for (int i = 0; i < 22; ++i) allowed.push_back(nickname_data + i);
    allowed.push_back(language_value);
    allowed.push_back(display_value);
    assert_only_value_changed(original, updated, allowed);
    check(sysconf::patch_settings(updated, overrides) == updated,
          "Repeated setup settings patch was not a no-op");

    auto incomplete = configured_sysconf();
    const int setup_start = get16(incomplete, 6);
    const int setup_value = setup_start + 1 + QByteArray("IPL.CD").size();
    incomplete[setup_value] = 0;
    rejects([&] { sysconf::patch_settings(incomplete, overrides); }, "an incomplete setup flag");
    overrides.nickname = "   ";
    rejects([&] { sysconf::patch_settings(original, overrides); }, "a blank nickname");
    overrides.nickname = "12345678901";
    rejects([&] { sysconf::patch_settings(original, overrides); }, "an oversized nickname");
    overrides.nickname = "Parents";
    overrides.language = "xx";
    rejects([&] { sysconf::patch_settings(original, overrides); }, "an unsupported language");
    auto missing_flag = make_sysconf({{"IPL.NIK", 2, 0, nickname_payload("Parents")},
                                      {"IPL.LNG", 3, 1}});
    rejects([&] { sysconf::patch_settings(missing_flag, overrides); }, "a missing setup flag");
  }

  void test_image_edit(const QByteArray& original) {
    QTemporaryDir scratch(QDir::current().filePath("build/sysconf-tests-XXXXXX"));
    check(scratch.isValid(), "Cannot create SYSCONF test directory");
    const auto path = scratch.filePath("working.bin");
    const QByteArray keys(0x400, '\0');
    const QByteArray settings(256, 's');
    {
      NandBin nand;
      UIDmap uid;
      check(nand.CreateNew(path, keys, QByteArray(0x108000, '\xff'), {}) &&
                initialize_system(nand, uid.Data()),
            "Cannot create SYSCONF test image");
      const QString menu = "/title/00000001/00000002";
      check(nand.CreateEntry("/title/00000001", 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                nand.CreateEntry(menu, 0, 0, NAND_DIR, NAND_RW, NAND_RW, NAND_READ) &&
                nand.CreateEntry(menu + "/data", 0x1000, 1, NAND_DIR, NAND_RW, 0, 0),
            "Cannot create settings test directory");
      const auto id = nand.CreateEntry(menu + "/data/setting.txt", 0x1000, 1,
                                       NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(id && nand.SetData(id, settings) && nand.WriteMetaData(), "Cannot write settings test file");
    }
    sysconf::DisplayOverrides aspect;
    aspect.widescreen = true;
    const auto empty_image = digest(path);
    rejects([&] { sysconf::set_display(path, aspect); }, "an image without SYSCONF");
    const auto sha256 = [](const QByteArray& bytes) {
      return QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
    };
    auto reference_keys = keys;
    QByteArray reference_header(sysconf::key_header_bytes, '\0');
    const QByteArray header("BackupMii v1, ConsoleID: 12345678\n");
    std::copy(header.cbegin(), header.cend(), reference_header.begin());
    reference_keys.replace(0, sysconf::key_header_bytes, reference_header);
    const sysconf::Source source{original, sha256(reference_keys.mid(sysconf::key_header_bytes)),
                                 sha256(settings)};
    auto invalid = source;
    invalid.keys_data_sha256 = sha256("unrelated keys");
    rejects([&] { sysconf::set_display(path, aspect, invalid); }, "an unrelated console");
    invalid = source;
    invalid.settings_sha256 = sha256("unrelated settings");
    rejects([&] { sysconf::set_display(path, aspect, invalid); }, "different console settings");
    check(digest(path) == empty_image, "Rejected SYSCONF edit changed the image");
    const auto initialized = sysconf::set_display(path, {}, source);
    check(initialized["initialized"].toBool() && initialized["changed"].toBool() &&
              initialized["sysconf_sha256"].toString() == QString::fromLatin1(sha256(original)),
          "New SYSCONF did not preserve the source bytes");
    const auto imported = digest(path);
    rejects([&] { sysconf::set_display(path, aspect, source); }, "a second import into an initialized image");
    check(digest(path) == imported, "Rejected SYSCONF reimport changed the image");
    const auto report = sysconf::set_display(path, aspect);
    check(report["changed"].toBool() && report["aspect_ratio"].toString() == "16:9",
          "Native display edit did not report its change");
    {
      NandBin nand;
      check(nand.SetPath(path) && nand.InitNand() &&
                nand.GetData("/shared2/sys/SYSCONF") == sysconf::patch_display(original, aspect),
            "Native display edit changed unselected SYSCONF bytes");
    }
    const auto once = digest(path);
    check(!sysconf::set_display(path, aspect)["changed"].toBool() && digest(path) == once,
          "Repeated display edit changed raw NAND bytes");
  }
} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    const auto original = make_sysconf(target_entries());
    const auto display = sysconf::read_display(original);
    check(display.widescreen == false && display.progressive_scan == true && display.pal60 == false,
          "SYSCONF display values were decoded incorrectly");
    const auto report = sysconf::display_report(display);
    check(report["aspect_ratio"].toString() == "4:3" &&
              report["progressive_scan"].toBool() && !report["pal60"].toBool(),
          "Display report did not describe SYSCONF values");

    const auto no_op = sysconf::patch_display(original, {});
    check(no_op == original, "Empty display overrides changed SYSCONF");

    const auto aspect_offset = get16(original, 6) + 1 + 6;
    const auto progressive_offset = get16(original, 8) + 1 + 7;
    const auto pal60_offset = get16(original, 10) + 1 + 7;

    sysconf::DisplayOverrides aspect;
    aspect.widescreen = true;
    const auto widescreen = sysconf::patch_display(original, aspect);
    check(static_cast<quint8>(widescreen[aspect_offset]) == 1 &&
              sysconf::read_display(widescreen).widescreen == true,
          "Aspect ratio patch did not update its field");
    assert_only_value_changed(original, widescreen, {aspect_offset});

    sysconf::DisplayOverrides progressive;
    progressive.progressive_scan = false;
    const auto no_progressive = sysconf::patch_display(original, progressive);
    check(static_cast<quint8>(no_progressive[progressive_offset]) == 0 &&
              sysconf::read_display(no_progressive).progressive_scan == false,
          "Progressive scan patch did not update its field");
    assert_only_value_changed(original, no_progressive, {progressive_offset});

    sysconf::DisplayOverrides pal60;
    pal60.pal60 = true;
    const auto enabled_pal60 = sysconf::patch_display(original, pal60);
    check(static_cast<quint8>(enabled_pal60[pal60_offset]) == 1 &&
              sysconf::read_display(enabled_pal60).pal60 == true,
          "PAL60 patch did not update its field");
    assert_only_value_changed(original, enabled_pal60, {pal60_offset});

    const auto sparse = make_sysconf({{"IPL.AR", 3, 0}, {"OTHER", 3, 0x5a}});
    const auto sparse_display = sysconf::read_display(sparse);
    check(!sparse_display.progressive_scan && !sparse_display.pal60 &&
              sysconf::display_report(sparse_display)["pal60"].isNull(),
          "Missing display preference was assigned a value");
    check(sysconf::read_display(sysconf::patch_display(sparse, aspect)).widescreen == true,
          "Existing aspect ratio could not be changed in a sparse SYSCONF");
    rejects([&] { sysconf::patch_display(sparse, progressive); }, "a request for a missing display field");

    auto bad_header = original;
    bad_header.replace(0, 4, "NOPE");
    rejects([&] { sysconf::read_display(bad_header); }, "an invalid SCv0 header");
    bad_header = original;
    bad_header.replace(0x3ffc, 4, "NOPE");
    rejects([&] { sysconf::read_display(bad_header); }, "an invalid SCed sentinel");

    auto bad_count = original;
    put16(bad_count, 4, 0xffff);
    rejects([&] { sysconf::read_display(bad_count); }, "an out-of-bounds entry count");
    auto bad_offset = original;
    put16(bad_offset, 6, 0x3ffd);
    rejects([&] { sysconf::read_display(bad_offset); }, "an out-of-bounds entry offset");
    auto reversed_offsets = original;
    put16(reversed_offsets, 8, get16(original, 6));
    rejects([&] { sysconf::read_display(reversed_offsets); }, "overlapping entry offsets");

    rejects([&] {
      sysconf::read_display(make_sysconf({{"IPL.AR", 3, 0}, {"IPL.AR", 3, 1},
                                          {"IPL.PGS", 7, 1}, {"IPL.E60", 7, 0}}));
    }, "duplicate display entries");
    rejects([&] {
      sysconf::read_display(make_sysconf({{"IPL.AR", 0, 0}, {"IPL.PGS", 7, 1},
                                          {"IPL.E60", 7, 0}}));
    }, "a display entry with the wrong type");
    rejects([&] {
      sysconf::read_display(make_sysconf({{"IPL.AR", 3, 2}, {"IPL.PGS", 7, 1},
                                          {"IPL.E60", 7, 0}}));
    },
            "an invalid aspect-ratio value");
    rejects([&] {
      sysconf::read_display(make_sysconf({{"IPL.AR", 3, 0}, {"IPL.PGS", 7, 2},
                                          {"IPL.E60", 7, 0}}));
    },
            "an invalid boolean value");

    test_missing_fields();
    test_setup_settings();
    test_image_edit(original);
    std::cout << "SYSCONF settings tests passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "SYSCONF display test failed: " << error.what() << '\n';
    return 1;
  }
}
