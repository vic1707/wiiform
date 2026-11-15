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

  auto make_wad() -> QByteArray {
    const QByteArray content(2 * 1024 * 1024, 'w');
    QByteArray tmd(0x1e4 + 0x24, '\0');
    put32(tmd, 0, ES_SIG_RSA2048);
    put64(tmd, 0x18c, wad_title_id);
    qToBigEndian<quint16>(wad_title_version, reinterpret_cast<uchar*>(tmd.data() + 0x1dc));
    qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(tmd.data() + 0x1de));
    auto* record = reinterpret_cast<uchar*>(tmd.data() + 0x1e4);
    qToBigEndian<quint32>(0x40, record);
    qToBigEndian<quint16>(7, record + 4);
    qToBigEndian<quint16>(1, record + 6);
    qToBigEndian<quint64>(content.size(), record + 8);
    tmd.replace(0x1f4, 20, GetSha1(content));

    QByteArray ticket(0x2a4, '\0');
    put32(ticket, 0, ES_SIG_RSA2048);
    put64(ticket, 0x1dc, wad_title_id);
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

  auto wad_digest(const QByteArray& wad) -> wiiform::Sha256 {
    const auto hash = QCryptographicHash::hash(wad, QCryptographicHash::Sha256);
    wiiform::Sha256 result{};
    std::copy_n(reinterpret_cast<const std::uint8_t*>(hash.constData()), result.size(), result.begin());
    return result;
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

  void test_capacity_helpers(std::span<std::uint8_t> image) {
    const auto before = QCryptographicHash::hash(
        QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                static_cast<int>(image.size())), QCryptographicHash::Sha256);
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
    const auto after = QCryptographicHash::hash(
        QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                static_cast<int>(image.size())), QCryptographicHash::Sha256);
    check(before == after, "Capacity checks changed the NAND buffer");
  }

  void test_wad_rejections(std::span<std::uint8_t> image, const QByteArray& wad) {
    const auto before = QCryptographicHash::hash(
        QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                static_cast<int>(image.size())), QCryptographicHash::Sha256);
    auto invalid_digest = wad_digest(wad);
    invalid_digest[0] ^= 0xff;
    rejects([&] {
      wiiform::core::install_wad(image,
          std::span(reinterpret_cast<const std::uint8_t*>(wad.constData()),
                    static_cast<std::size_t>(wad.size())),
          invalid_digest, wiiform::WadPolicy::legacy_channel);
    }, "SHA-256");
    check(before == QCryptographicHash::hash(
              QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                      static_cast<int>(image.size())), QCryptographicHash::Sha256),
          "Invalid WAD digest changed the NAND buffer");
    auto invalid_content = wad;
    // WAD data begins after the aligned header, certificate, ticket, and TMD sections.
    constexpr int content_offset = 0x40 + 0x40 + 0x2c0 + 0x240;
    invalid_content[content_offset + 20] = char(invalid_content.at(content_offset + 20) ^ 1);
    rejects([&] {
      wiiform::core::install_wad(image,
          std::span(reinterpret_cast<const std::uint8_t*>(invalid_content.constData()),
                    static_cast<std::size_t>(invalid_content.size())),
          wad_digest(invalid_content), wiiform::WadPolicy::legacy_channel);
    }, "WAD content validation failed");
    check(before == QCryptographicHash::hash(
              QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                      static_cast<int>(image.size())), QCryptographicHash::Sha256),
          "Invalid WAD content changed the NAND buffer");
    rejects([&] {
      wiiform::core::install_wad(image,
          std::span(reinterpret_cast<const std::uint8_t*>(wad.constData()),
                    static_cast<std::size_t>(wad.size())),
          wad_digest(wad), wiiform::WadPolicy::legacy_channel);
    }, "Insufficient NAND space");
    check(before == QCryptographicHash::hash(
              QByteArray::fromRawData(reinterpret_cast<const char*>(image.data()),
                                      static_cast<int>(image.size())), QCryptographicHash::Sha256),
          "WAD capacity failure changed the NAND buffer");
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    if (argc == 3 && QString::fromLocal8Bit(argv[1]) == "--make-fixtures") {
      make_fixtures(QString::fromLocal8Bit(argv[2]));
      return 0;
    }
    check(argc == 1, "Usage: nand-buffer-regressions [--make-fixtures DIR]");
    std::async(std::launch::async, [] {
      rejects([] { wiiform::core::remove_title({}, 0); }, "QCoreApplication thread");
    }).get();
    QTemporaryDir temporary(QDir::current().filePath("build/nand-buffer-regressions-XXXXXX"));
    check(temporary.isValid(), "Cannot create buffer regression directory");
    const auto nand_path = QDir(temporary.path()).filePath("nand.bin");
    create_near_full_nand(nand_path);
    auto image = read_bytes(nand_path, nand_file_size);
    const auto wad = make_wad();
    test_capacity_helpers(image);
    test_wad_rejections(image, wad);
    std::cout << "NAND buffer capacity and rejection checks passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "NAND buffer regressions failed: " << error.what() << '\n';
    return 1;
  }
}
