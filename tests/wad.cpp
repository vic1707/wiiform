// SPDX-License-Identifier: GPL-2.0-only
#include <WadPackage.hpp>
#include <tiktmd.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QTemporaryDir>
#include <functional>
#include <iostream>
#include <stdexcept>

namespace {
  constexpr quint64 title_id = 0x0001000148414243ull;
  constexpr quint16 title_version = 17;

  struct Content {
    QByteArray bytes;
    quint16 index;
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
    throw std::runtime_error(std::string("WAD reader accepted ") + label);
  }

  auto put32(QByteArray& bytes, int offset, quint32 value) -> void {
    qToBigEndian<quint32>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto put64(QByteArray& bytes, int offset, quint64 value) -> void {
    qToBigEndian<quint64>(value, reinterpret_cast<uchar*>(bytes.data() + offset));
  }

  auto round_up(int value, int alignment) -> int {
    return ((value + alignment - 1) / alignment) * alignment;
  }

  auto make_wad(const QList<Content>& contents) -> QByteArray {
    QByteArray tmd(0x1e4 + contents.size() * 0x24, '\0');
    put32(tmd, 0, ES_SIG_RSA2048);
    put64(tmd, 0x18c, title_id);
    qToBigEndian<quint16>(title_version, reinterpret_cast<uchar*>(tmd.data() + 0x1dc));
    qToBigEndian<quint16>(contents.size(), reinterpret_cast<uchar*>(tmd.data() + 0x1de));

    QByteArray ticket(0x2a4, '\0');
    put32(ticket, 0, ES_SIG_RSA2048);
    put64(ticket, 0x1dc, title_id);

    Ticket parsed_ticket(ticket, false);
    QByteArray encrypted_contents;
    for (int index = 0; index < contents.size(); ++index) {
      auto* record = reinterpret_cast<uchar*>(tmd.data() + 0x1e4 + index * 0x24);
      qToBigEndian<quint32>(0x40 + index, record);
      qToBigEndian<quint16>(contents[index].index, record + 4);
      qToBigEndian<quint16>(1, record + 6);
      qToBigEndian<quint64>(contents[index].bytes.size(), record + 8);
      tmd.replace(0x1f4 + index * 0x24, 20, GetSha1(contents[index].bytes));
    }
    Tmd metadata(tmd);
    AesSetKey(parsed_ticket.DecryptedKey());
    for (int index = 0; index < contents.size(); ++index) {
      const auto padded = PaddedByteArray(contents[index].bytes, 0x40);
      encrypted_contents += PaddedByteArray(AesEncrypt(metadata.Index(index), padded), 0x40);
    }

    const QByteArray certs("synthetic certificate data");
    const QByteArray footer("synthetic wad footer");
    QByteArray header(0x40, '\0');
    put32(header, 0, 0x20);
    put32(header, 4, 0x49730000);
    put32(header, 8, certs.size());
    put32(header, 0x10, ticket.size());
    put32(header, 0x14, tmd.size());
    put32(header, 0x18, encrypted_contents.size());
    put32(header, 0x1c, footer.size());
    return header + PaddedByteArray(certs, 0x40) + PaddedByteArray(ticket, 0x40) +
           PaddedByteArray(tmd, 0x40) + encrypted_contents + footer;
  }

  auto write_file(const QString& path, const QByteArray& bytes) -> void {
    QFile file(path);
    check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly) &&
              file.write(bytes) == bytes.size() && file.flush(),
          "Cannot write synthetic WAD fixture");
  }

  auto write_fixture(const QDir& directory, const QString& name,
                     const QByteArray& bytes) -> QString {
    const auto path = directory.filePath(name + ".wad");
    write_file(path, bytes);
    return path;
  }

  auto sha256(const QByteArray& bytes) -> QByteArray {
    return QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
  }
} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    QTemporaryDir temporary(QDir::current().filePath("build/wad-tests-XXXXXX"));
    check(temporary.isValid(), "Cannot create WAD test directory");
    const QDir directory(temporary.path());
    const QList<Content> contents{{QByteArray("first content payload"), 7},
                                  {QByteArray("another payload with a sparse index"), 42}};
    const auto valid_bytes = make_wad(contents);
    const auto valid_path = write_fixture(directory, "valid", valid_bytes);
    const auto actual_hash = sha256(valid_bytes);
    const auto package = wad_package::read(valid_path, actual_hash.toUpper());
    check(package.sha256 == actual_hash && !package.nintendo_signed(),
          "Unsigned synthetic package reporting changed");
    check(package.job.tid == title_id && package.job.version == title_version && package.job.decrypt &&
              package.job.data.size() == contents.size() + 2,
          "WAD title metadata was not reconstructed");
    check(package.job.data[2] == contents[0].bytes && package.job.data[3] == contents[1].bytes,
          "WAD contents were not decrypted in TMD order");

    const auto frontend = QDir(QCoreApplication::applicationDirPath()).filePath("wii-nand");
    QProcess inspect;
    inspect.start(frontend, {"inspect-wad", valid_path});
    check(inspect.waitForFinished() && inspect.exitStatus() == QProcess::NormalExit && inspect.exitCode() == 0,
          "Cannot inspect a valid WAD through the CLI");
    const auto report = QJsonDocument::fromJson(inspect.readAllStandardOutput()).object();
    check(report["sha256"].toString() == QString::fromLatin1(actual_hash) &&
              report["tmd_sha256"].toString() == QString::fromLatin1(sha256(package.job.data[0])) &&
              report["content_count"].toInt() == contents.size() && report["nintendo_signed"].isBool() &&
              !report["nintendo_signed"].toBool(), "WAD inspection report did not describe the package");

    QProcess inspect_stream;
    inspect_stream.start(frontend, {"inspect-wad", "--allow-unverified-signatures", "--sha256",
                                    QString::fromLatin1(actual_hash), "-"});
    check(inspect_stream.waitForStarted() && inspect_stream.write(valid_bytes) == valid_bytes.size(),
          "Cannot send a WAD through stdin");
    inspect_stream.closeWriteChannel();
    check(inspect_stream.waitForFinished() && inspect_stream.exitStatus() == QProcess::NormalExit &&
              inspect_stream.exitCode() == 0 && QJsonDocument::fromJson(inspect_stream.readAllStandardOutput()).object() == report,
          "Inspection of the pinned WAD from stdin did not match the file report");

    const auto target = directory.filePath("unused.nand");
    QProcess install;
    install.start(frontend, {"install-wad", "--in-place", "--sha256", QString::fromLatin1(actual_hash),
                             target, valid_path});
    check(install.waitForFinished() && install.exitStatus() == QProcess::NormalExit && install.exitCode() != 0,
          "Unsigned WAD passed the CLI signature gate");
    const auto install_error = install.readAllStandardError();
    if (!install_error.contains("genuinely Nintendo-signed"))
      throw std::runtime_error("Unexpected WAD signature rejection: " + install_error.toStdString());
    check(!QFileInfo::exists(target), "Rejected package created a NAND image");

    rejects([&] { wad_package::read(valid_path, QByteArray(64, '0')); }, "wrong package digest");

    auto unpadded_length = valid_bytes;
    put32(unpadded_length, 0x18, round_up(contents[0].bytes.size(), 0x40) + contents[1].bytes.size());
    const auto unpadded_package = wad_package::read(write_fixture(directory, "unpadded-data-length", unpadded_length));
    check(unpadded_package.job.data == package.job.data, "Unpadded header length changed the package contents");

    auto bad = valid_bytes;
    put32(bad, 8, 0xffffffffu);
    rejects([&] { wad_package::read(write_fixture(directory, "large-cert", bad)); },
            "oversized section length");

    bad = valid_bytes;
    put32(bad, 0x0c, 0x40);
    rejects([&] { wad_package::read(write_fixture(directory, "nonzero-reserved", bad)); },
            "nonzero reserved header field");

    bad = valid_bytes;
    put32(bad, 0x10, 0x280);
    rejects([&] { wad_package::read(write_fixture(directory, "short-ticket", bad)); },
            "noncanonical ticket size");

    bad = valid_bytes;
    put32(bad, 0x10, 0xffffffffu);
    rejects([&] { wad_package::read(write_fixture(directory, "large-ticket", bad)); },
            "oversized ticket length");

    bad = valid_bytes;
    put32(bad, 0x14, 0x1e4);
    rejects([&] { wad_package::read(write_fixture(directory, "short-table", bad)); },
            "truncated content table");

    bad = valid_bytes;
    const auto ticket_offset = 0x40 + round_up(26, 0x40);
    put64(bad, ticket_offset + 0x1dc, title_id + 1);
    rejects([&] { wad_package::read(write_fixture(directory, "ticket-title", bad)); },
            "ticket and TMD title mismatch");

    bad = valid_bytes;
    bad[ticket_offset + 0x1f1] = 1;
    const auto alternate_key = write_fixture(directory, "alternate-key", bad);
    const auto legacy = wad_package::Policy::LegacyChannel;
    rejects([&] { wad_package::read(alternate_key, sha256(bad), legacy); }, "alternate common key in legacy mode");

    auto legacy_bytes = valid_bytes;
    legacy_bytes[ticket_offset + 0x1f1] = char(0xcd);
    const auto legacy_path = write_fixture(directory, "legacy-selector", legacy_bytes);
    rejects([&] { wad_package::read(legacy_path); }, "invalid key selector in standard mode");
    rejects([&] { wad_package::read(legacy_path, {}, legacy); }, "legacy package without a pinned hash");
    const auto compatible = wad_package::read(legacy_path, sha256(legacy_bytes), legacy);
    check(compatible.job.data == package.job.data && compatible.source_common_key_index == 0xcd &&
              compatible.source_ticket_sha256 != package.source_ticket_sha256 && !compatible.nintendo_signed(),
          "Legacy key handling changed more than the invalid selector");

    bad = valid_bytes;
    put32(bad, ticket_offset + 0x1d8, 1234);
    rejects([&] { wad_package::read(write_fixture(directory, "console-bound", bad)); },
            "console-bound ticket");

    bad = valid_bytes;
    const auto tmd_offset = ticket_offset + round_up(0x2a4, 0x40);
    const auto contents_offset = tmd_offset + round_up(0x1e4 + contents.size() * 0x24, 0x40);
    bad[contents_offset + 4] = char(bad.at(contents_offset + 4) ^ 0x40);
    rejects([&] { wad_package::read(write_fixture(directory, "corrupt-content", bad)); },
            "content hash mismatch");

    bad = valid_bytes.left(valid_bytes.size() - 3);
    rejects([&] { wad_package::read(write_fixture(directory, "truncated", bad)); },
            "truncated content or footer");

    bad = valid_bytes;
    put32(bad, 4, 0x426b0000);
    rejects([&] { wad_package::read(write_fixture(directory, "boot2", bad)); },
            "non-installable boot2 WAD");

    std::cout << "WAD bounds, identity, digest and content checks passed\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "WAD reader tests failed: " << error.what() << '\n';
    return 1;
  }
}
