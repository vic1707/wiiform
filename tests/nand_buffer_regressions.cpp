// SPDX-License-Identifier: GPL-2.0-only
#include <Factory.hpp>
#include <Titles.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>
#include <wiiform/write.hpp>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <uidmap.h>
#include <algorithm>
#include <future>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <vector>

namespace {
  constexpr quint64 wad_title_id = 0x0001000148414243ull;
  constexpr quint16 wad_title_version = 17;
  constexpr quint64 nand_file_size = wiiform::nand_size;

  void check(bool value, const char* message) {
    if (!value) throw std::runtime_error(message);
  }

  template<class Operation>
  void rejects(Operation operation, const char* expected_text) {
    try {
      operation();
    } catch (const std::exception& error) {
      check(std::string(error.what()).find(expected_text) != std::string::npos, error.what());
      return;
    }
    throw std::runtime_error("Expected the native NAND operation to reject the request");
  }

  auto round_up(quint64 size, quint64 alignment) -> quint64 {
    return (size + alignment - 1) & ~(alignment - 1);
  }

  void put32(QByteArray& bytes, int offset, quint32 value) {
    qToBigEndian<quint32>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  void put64(QByteArray& bytes, int offset, quint64 value) {
    qToBigEndian<quint64>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto make_wad(char content_byte = 'w', quint16 version = wad_title_version,
                quint64 title_id = wad_title_id, quint16 content_type = 1,
                quint16 content_index = 7,
                const QByteArray& first_content = {}, quint8 tmd_marker = 0) -> QByteArray {
    QByteArray content(2 * 1024 * 1024, content_byte);
    if (!first_content.isEmpty()) {
      check(first_content.size() <= content.size(), "Synthetic first content header is too large");
      content.replace(0, first_content.size(), first_content);
    }
    QByteArray tmd(0x1e4 + 0x24, '\0');
    put32(tmd, 0, ES_SIG_RSA2048);
    tmd[0x183] = static_cast<char>(tmd_marker);
    put64(tmd, 0x18c, title_id);
    qToBigEndian<quint16>(version, reinterpret_cast<uchar*>(tmd.data() + 0x1dc));
    qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(tmd.data() + 0x1de));
    auto* record = reinterpret_cast<uchar*>(tmd.data() + 0x1e4);
    qToBigEndian<quint32>(0x40, record);
    qToBigEndian<quint16>(content_index, record + 4);
    qToBigEndian<quint16>(content_type, record + 6);
    qToBigEndian<quint64>(content.size(), record + 8);
    tmd.replace(0x1f4, 20, GetSha1(content));

    QByteArray ticket(0x2a4, '\0');
    put32(ticket, 0, ES_SIG_RSA2048);
    put64(ticket, 0x1dc, title_id);
    Ticket parsed_ticket(ticket, false);
    Tmd metadata(tmd);
    AesSetKey(parsed_ticket.DecryptedKey());
    const auto encrypted = PaddedByteArray(
        AesEncrypt(metadata.Index(0), PaddedByteArray(content, 0x40)), 0x40);

    const QByteArray certs("synthetic certificate bytes");
    const QByteArray footer("synthetic WAD footer");
    QByteArray header(0x40, '\0');
    put32(header, 0, 0x20);
    put32(header, 4, 0x49730000);
    put32(header, 8, certs.size());
    put32(header, 0x10, ticket.size());
    put32(header, 0x14, tmd.size());
    put32(header, 0x18, encrypted.size());
    put32(header, 0x1c, footer.size());
    return header + PaddedByteArray(certs, 0x40) + PaddedByteArray(ticket, 0x40) +
           PaddedByteArray(tmd, 0x40) + encrypted + footer;
  }

  auto make_iosinfo(quint8 base, quint32 version = 11,
                    const QByteArray& name = "d2x", const QByteArray& suffix = "beta3") -> QByteArray {
    QByteArray record(48, '\0');
    put32(record, 0, 0x1ee7c105);
    put32(record, 4, 1);
    put32(record, 8, version);
    put32(record, 12, base);
    record.replace(16, qMin(name.size(), 16), name.left(16));
    record.replace(32, qMin(suffix.size(), 16), suffix.left(16));
    return record;
  }

  auto make_cios_wad(quint8 slot = 249, quint8 base = 56, quint16 revision = 65535,
                     quint8 marker = 0, bool valid_iosinfo = true,
                     const QByteArray& suffix = "beta3") -> QByteArray {
    const auto info = valid_iosinfo ? make_iosinfo(base, 11, "d2x", suffix) : QByteArray(48, '\0');
    return make_wad('c', revision, 0x0000000100000000ull | slot, 1, 0, info, marker);
  }

  auto wad_digest(const QByteArray& wad) -> wiiform::Sha256 {
    const auto hash = QCryptographicHash::hash(wad, QCryptographicHash::Sha256);
    wiiform::Sha256 result{};
    std::copy_n(reinterpret_cast<const std::uint8_t*>(hash.constData()), result.size(), result.begin());
    return result;
  }

  auto wad_bytes(const QByteArray& wad) -> std::span<const std::uint8_t> {
    return {reinterpret_cast<const std::uint8_t*>(wad.constData()),
            static_cast<std::size_t>(wad.size())};
  }

  auto buffer_digest(std::span<const std::uint8_t> image) -> QByteArray {
    return QCryptographicHash::hash(
        QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                static_cast<int>(image.size())),
        QCryptographicHash::Sha256);
  }

  template<class Operation>
  void rejects_unchanged(std::span<std::uint8_t> image, Operation operation,
                         const char* expected_text, const char* message) {
    const auto before = buffer_digest(image);
    try {
      operation();
    } catch (const std::exception& error) {
      check(std::string(error.what()).find(expected_text) != std::string::npos, error.what());
      check(before == buffer_digest(image), message);
      return;
    }
    throw std::runtime_error("Expected the native cIOS operation to reject the request");
  }

  auto file_digest(const QString& path) -> QByteArray {
    QFile file(path);
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(file.open(QIODevice::ReadOnly) && hash.addData(&file), "Cannot hash synthetic NAND");
    return hash.result();
  }

  auto read_bytes(const QString& path, quint64 exact_size) -> std::vector<std::uint8_t> {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly) && quint64(file.size()) == exact_size,
          "Synthetic NAND has an unexpected size");
    std::vector<std::uint8_t> result(static_cast<std::size_t>(exact_size));
    quint64 offset = 0;
    while (offset < exact_size) {
      const auto amount = file.read(reinterpret_cast<char*>(result.data() + offset),
                                    static_cast<qint64>(qMin<quint64>(1024 * 1024, exact_size - offset)));
      check(amount > 0, "Cannot read synthetic NAND bytes");
      offset += static_cast<quint64>(amount);
    }
    return result;
  }

  void write_bytes(const QString& path, const QByteArray& bytes) {
    QFile file(path);
    check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly) &&
              file.write(bytes) == bytes.size() && file.flush(),
          "Cannot write generated test fixture");
  }

  void create_near_full_nand(const QString& path) {
    QList<quint16> bad_blocks;
    for (quint16 block = 9; block < 4080; ++block) bad_blocks.append(block);
    NandBin nand;
    UIDmap uid;
    check(nand.CreateNew(path, QByteArray(0x400, '\0'),
                         QByteArray(0x108000, '\xff'), bad_blocks) &&
              initialize_system(nand, uid.Data()) && nand.WriteMetaData(),
          "Cannot construct nearly full synthetic NAND");

    NandBin verify;
    check(verify.SetPath(path) && verify.InitNand(), "Cannot reopen synthetic NAND");
    const auto free_clusters = verify.GetFats().count(0xfffe);
    check(free_clusters > 2 && free_clusters < 64,
          "Near-full synthetic NAND does not have the expected remaining capacity");
  }

  void create_empty_nand(const QString& path) {
    NandBin nand;
    UIDmap uid;
    check(nand.CreateNew(path, QByteArray(0x400, '\0'),
                         QByteArray(0x108000, '\xff')) &&
              initialize_system(nand, uid.Data()) && nand.WriteMetaData(),
          "Cannot construct an empty synthetic NAND");
  }

  auto make_fixtures(const QString& directory) -> void {
    check(QDir().mkpath(directory), "Cannot create fixture directory");
    const QDir output(directory);
    const auto nand_path = output.filePath("nand.bin");
    const auto wad_path = output.filePath("fixture.wad");
    create_near_full_nand(nand_path);
    const auto wad = make_wad();
    write_bytes(wad_path, wad);
    const auto hash = QCryptographicHash::hash(wad, QCryptographicHash::Sha256).toHex();
    write_bytes(output.filePath("fixture.wad.sha256"), hash + '\n');
  }

  auto make_cios_fixture(const QString& directory) -> void {
    check(QDir().mkpath(directory), "Cannot create cIOS fixture directory");
    const QDir output(directory);
    const auto wad = make_cios_wad();
    write_bytes(output.filePath("cios.wad"), wad);
    write_bytes(output.filePath("cios.wad.sha256"),
                QCryptographicHash::hash(wad, QCryptographicHash::Sha256).toHex() + '\n');
    create_empty_nand(output.filePath("empty-nand.bin"));
  }

  void test_capacity_helpers(std::span<std::uint8_t> image) {
    const auto before = buffer_digest(image);
    {
      working_image::Image working(image);
      const std::unique_ptr<QTreeWidgetItem> tree(working.nand.GetTree());
      const auto free_clusters = working.nand.GetFats().count(0xfffe);
      rejects([&] {
        working_image::require_capacity(working.nand, tree->child(0), working.layout.orphan_entries,
                                        0, static_cast<quint64>(free_clusters));
      }, "one cluster is reserved");
      rejects([&] {
        working_image::require_capacity(working.nand, tree->child(0), working.layout.orphan_entries,
                                        0x1800, 0);
      }, "FST entries");
    }
    const auto after = buffer_digest(image);
    check(before == after, "Capacity checks changed the NAND buffer");
  }

  void test_wad_rejections(std::span<std::uint8_t> image, const QByteArray& wad) {
    const auto before = buffer_digest(image);
    auto invalid_digest = wad_digest(wad);
    invalid_digest[0] ^= 0xff;
    rejects([&] {
      wiiform::core::install_wad(image, wad_bytes(wad), invalid_digest,
                                 wiiform::WadPolicy::legacy_channel);
    }, "SHA-256");
    check(before == buffer_digest(image), "Invalid WAD digest changed the NAND buffer");
    auto invalid_content = wad;
    // WAD data begins after the aligned header, certificate, ticket, and TMD sections.
    constexpr int content_offset = 0x40 + 0x40 + 0x2c0 + 0x240;
    invalid_content[content_offset + 20] = char(invalid_content.at(content_offset + 20) ^ 1);
    rejects([&] {
      wiiform::core::install_wad(image, wad_bytes(invalid_content), wad_digest(invalid_content),
                                 wiiform::WadPolicy::legacy_channel);
    }, "WAD content validation failed");
    check(before == buffer_digest(image), "Invalid WAD content changed the NAND buffer");
    rejects([&] {
      wiiform::core::install_wad(image, wad_bytes(wad), wad_digest(wad),
                                 wiiform::WadPolicy::legacy_channel);
    }, "Insufficient NAND space");
    check(before == buffer_digest(image), "WAD capacity failure changed the NAND buffer");
  }

  void test_ios_wad_policy(std::span<std::uint8_t> image) {
    using wiiform::WadPolicy;
    const auto before = buffer_digest(image);
    const auto ios = make_wad('i', 1034, 0x0000000100000009ull);
    const auto install = [&](const QByteArray& wad, WadPolicy policy) {
      return wiiform::core::install_wad(image, wad_bytes(wad), wad_digest(wad), policy);
    };
    rejects([&] { install(ios, WadPolicy::standard); }, "must be genuinely Nintendo-signed");
    rejects([&] { install(ios, WadPolicy::legacy_channel); }, "only downloadable channels");

    auto vwii = ios;
    constexpr int tmd_offset = 0x40 + 0x40 + 0x2c0;
    vwii[tmd_offset + 0x183] = char(1);
    rejects([&] { install(vwii, WadPolicy::standard); }, "vWii WADs are unsupported");

    const auto bootmii = make_wad('i', 65280, 0x00000001000000feull);
    rejects([&] { install(bootmii, WadPolicy::standard); }, "IOS slots 3 through 253");
    check(before == buffer_digest(image), "Unsupported IOS WAD changed the NAND buffer");
  }

  void test_wad_installation(const QString& temporary_path) {
    const auto nand_path = QDir(temporary_path).filePath("empty-nand.bin");
    create_empty_nand(nand_path);
    auto image = read_bytes(nand_path, nand_file_size);
    const auto wad = make_wad();
    const auto install = [&](const QByteArray& package) {
      return wiiform::core::install_wad(image, wad_bytes(package), wad_digest(package),
                                        wiiform::WadPolicy::legacy_channel);
    };

    const auto installed = install(wad);
    check(installed.title_id == wad_title_id && installed.version == wad_title_version &&
              !installed.already_present,
          "Fresh WAD install returned the wrong title identity or outcome");
    const auto installed_hash = buffer_digest(image);

    const auto matched = install(wad);
    check(matched.title_id == wad_title_id && matched.version == wad_title_version &&
              matched.already_present,
          "Exact WAD reapplication was not reported as already installed");
    check(installed_hash == buffer_digest(image), "Exact WAD reapplication changed the NAND buffer");

    const auto conflicting_wad = make_wad('x');
    const auto before_conflict = buffer_digest(image);
    rejects([&] {
      install(conflicting_wad);
    }, "Installed title conflicts with WAD title metadata");
    check(before_conflict == buffer_digest(image),
          "Conflicting WAD changed the NAND buffer");

    const auto version_conflict = make_wad('w', wad_title_version + 1);
    rejects([&] { install(version_conflict); }, "Installed title conflicts with WAD title metadata");
    check(before_conflict == buffer_digest(image),
          "Title-version conflict changed the NAND buffer");

    {
      working_image::Image working(image);
      check(working.nand.SetData("/title/00010001/48414243/content/00000040.app",
                                 QByteArray(2 * 1024 * 1024, 'x')) &&
                working.nand.WriteMetaData(),
            "Cannot corrupt installed title content");
    }
    const auto before_corrupt_content = buffer_digest(image);
    rejects([&] { install(wad); }, "Installed title content conflicts with WAD");
    check(before_corrupt_content == buffer_digest(image),
          "Corrupt installed title content changed during reapplication");

    {
      working_image::Image working(image);
      check(working.nand.Delete("/title/00010001/48414243/content/00000040.app") &&
                working.nand.WriteMetaData(),
            "Cannot make an incomplete installed WAD fixture");
    }
    const auto before_incomplete = buffer_digest(image);
    rejects([&] {
      install(wad);
    }, "Installed title is incomplete or inconsistent with WAD");
    check(before_incomplete == buffer_digest(image), "Incomplete WAD reapplication changed the NAND buffer");
  }

  void test_shared_wad_matching(const QString& temporary_path) {
    const auto nand_path = QDir(temporary_path).filePath("shared-empty-nand.bin");
    create_empty_nand(nand_path);
    auto image = read_bytes(nand_path, nand_file_size);
    constexpr quint64 first_title_id = 0x0001000148414244ull;
    constexpr quint64 second_title_id = 0x0001000148414245ull;
    const auto first_wad = make_wad('s', wad_title_version, first_title_id, 0x8001);
    const auto second_wad = make_wad('s', wad_title_version, second_title_id, 0x8001);
    const auto install = [&](const QByteArray& wad) {
      return wiiform::core::install_wad(image, wad_bytes(wad), wad_digest(wad),
                                        wiiform::WadPolicy::legacy_channel);
    };

    check(!install(first_wad).already_present && !install(second_wad).already_present,
          "Shared WAD fixtures did not install");
    const auto before_match = buffer_digest(image);
    const auto matched = install(second_wad);
    check(matched.title_id == second_title_id && matched.already_present,
          "A title using shared content was not recognized as already installed");
    check(before_match == buffer_digest(image),
          "Shared-content WAD reapplication changed the NAND buffer");

    {
      working_image::Image working(image);
      check(working.nand.SetData("/shared1/00000000.app", QByteArray(2 * 1024 * 1024, 'x')) &&
                working.nand.WriteMetaData(),
            "Cannot corrupt shared-content fixture");
    }
    const auto before_corrupt = buffer_digest(image);
    rejects([&] { install(second_wad); }, "Installed title content conflicts with WAD");
    check(before_corrupt == buffer_digest(image), "Invalid shared content changed during WAD reapplication");
  }

  void test_cios_installation(const QString& temporary_path) {
    const auto nand_path = QDir(temporary_path).filePath("cios-empty-nand.bin");
    create_empty_nand(nand_path);
    auto image = read_bytes(nand_path, nand_file_size);
    const auto wad = make_cios_wad();
    const wiiform::CiosOptions options{249, 56, 65535, false};
    const auto install = [&](const QByteArray& package, const wiiform::CiosOptions& target) {
      return wiiform::core::install_cios(image, wad_bytes(package), wad_digest(package), target);
    };

    const auto installed = install(wad, options);
    check(installed.title_id == 0x00000001000000f9ull && installed.version == 65535 &&
              !installed.already_present,
          "Fresh cIOS install returned the wrong title identity or outcome");
    const auto after_install = buffer_digest(image);
    const auto matched = install(wad, options);
    check(matched.title_id == installed.title_id && matched.version == installed.version &&
              matched.already_present,
          "Exact cIOS reapplication was not reported as already installed");
    check(after_install == buffer_digest(image), "Exact cIOS reapplication changed the NAND buffer");

    const auto conflict = make_cios_wad(249, 56, 65535, 0, true, "beta4");
    rejects_unchanged(image, [&] { install(conflict, options); }, "Installed title",
                      "Conflicting cIOS WAD changed the NAND buffer");

    const wiiform::CiosOptions wrong_slot{250, 56, 65535, false};
    rejects_unchanged(image, [&] { install(wad, wrong_slot); },
                      "WAD does not match requested cIOS slot",
                      "Wrong-slot cIOS WAD changed the NAND buffer");
    const wiiform::CiosOptions wrong_base{249, 58, 65535, false};
    rejects_unchanged(image, [&] { install(wad, wrong_base); },
                      "valid d2x IOSINFO header for the requested base",
                      "Wrong-base cIOS WAD changed the NAND buffer");
    const wiiform::CiosOptions wrong_revision{249, 56, 21010, false};
    rejects_unchanged(image, [&] { install(wad, wrong_revision); },
                      "WAD does not match requested cIOS revision",
                      "Wrong-revision cIOS WAD changed the NAND buffer");

    const auto vwii = make_cios_wad(249, 56, 65535, 1);
    rejects_unchanged(image, [&] { install(vwii, options); }, "vWii WADs are unsupported",
                      "vWii cIOS WAD changed the NAND buffer");
    const auto no_iosinfo = make_cios_wad(249, 56, 65535, 0, false);
    rejects_unchanged(image, [&] { install(no_iosinfo, options); },
                      "valid d2x IOSINFO header for the requested base",
                      "cIOS WAD without IOSINFO changed the NAND buffer");

    const wiiform::CiosOptions replace{249, 56, 65535, true};
    rejects_unchanged(image, [&] { install(conflict, replace); },
                      "not an authentic Nintendo stub",
                      "Replacing a non-stock cIOS changed the NAND buffer");
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    if (argc == 3 && QString::fromLocal8Bit(argv[1]) == "--make-fixtures") {
      make_fixtures(QString::fromLocal8Bit(argv[2]));
      return 0;
    }
    if (argc == 3 && QString::fromLocal8Bit(argv[1]) == "--make-empty-fixture") {
      create_empty_nand(QString::fromLocal8Bit(argv[2]));
      return 0;
    }
    if (argc == 3 && QString::fromLocal8Bit(argv[1]) == "--make-cios-fixture") {
      make_cios_fixture(QString::fromLocal8Bit(argv[2]));
      return 0;
    }
    check(argc == 1,
          "Usage: nand-buffer-regressions [--make-fixtures DIR] [--make-empty-fixture PATH] "
          "[--make-cios-fixture DIR]");
    std::async(std::launch::async, [] {
      rejects([] { wiiform::core::remove_title({}, 0); }, "QCoreApplication thread");
    }).get();
    QTemporaryDir temporary(QDir::current().filePath("build/nand-buffer-regressions-XXXXXX"));
    check(temporary.isValid(), "Cannot create buffer regression directory");
    {
      const auto nand_path = QDir(temporary.path()).filePath("nand.bin");
      create_near_full_nand(nand_path);
      auto image = read_bytes(nand_path, nand_file_size);
      test_capacity_helpers(image);
      test_wad_rejections(image, make_wad());
      test_ios_wad_policy(image);
    }
    test_wad_installation(temporary.path());
    test_shared_wad_matching(temporary.path());
    test_cios_installation(temporary.path());
    std::cout << "NAND buffer capacity, rejection, and WAD matching checks passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "NAND buffer regressions failed: " << error.what() << '\n';
    return 1;
  }
}
