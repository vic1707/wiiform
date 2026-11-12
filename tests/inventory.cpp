// SPDX-License-Identifier: GPL-2.0-only
#include <PriiloaderSettings.hpp>
#include <nandbin.h>
#include <sharedcontentmap.h>
#include <tiktmd.h>
#include <tools.h>
#include <uidmap.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QTemporaryDir>
#include <QtEndian>

#include <algorithm>
#include <filesystem>
#include <iostream>
#include <memory>
#include <stdexcept>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void test_priiloader_settings_writer() {
    const auto defaults = priiloader::default_settings();
    check(defaults == QByteArray::fromHex("03000000000a0000000000000000000000000000000101000000000000000000"),
          "Priiloader first-boot defaults differ from the pinned layout");
    auto source = defaults;
    source[31] = 'p';
    const auto edited = priiloader::patch_settings(source, {{"return_to", "priiloader"},
                                                          {"black_background", "off"},
                                                          {"preferred_mount_point", "usb"}});
    auto expected = source;
    expected[8] = 1;
    expected[22] = 0;
    expected[27] = 2;
    check(edited == expected, "Priiloader settings edit did not preserve other bytes and padding");
    const auto values = priiloader::inspect_settings(edited).value("values").toObject();
    check(values["return_to"].toString() == "priiloader" &&
              values["black_background"].isBool() && !values["black_background"].toBool() &&
              values["preferred_mount_point"].toString() == "usb",
          "Priiloader settings write and read disagree");

    for (const auto& bad : {QMap<QString, QString>{{"autoboot", "error"}},
                            QMap<QString, QString>{{"use_system_menu_ios", "off"}},
                            QMap<QString, QString>{{"passcheck_menu", "on"}},
                            QMap<QString, QString>{{"stop_disc", "true"}}}) {
      bool rejected = false;
      try {
        priiloader::patch_settings(source, bad);
      } catch (const std::runtime_error&) {
        rejected = true;
      }
      check(rejected, "Priiloader accepted an invalid or read-only setting");
    }
  }

  auto read_file(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot read synthetic inventory fixture");
    const auto bytes = file.readAll();
    check(file.error() == QFileDevice::NoError, "Cannot finish synthetic fixture read");
    return bytes;
  }

  auto read_region(const QString& path, qint64 offset, qint64 size) -> QByteArray {
    QFile file(path);
    check(offset >= 0 && size >= 0 && file.open(QIODevice::ReadOnly) && file.seek(offset),
          "Cannot seek synthetic NAND fixture");
    const auto bytes = file.read(size);
    check(bytes.size() == size && file.error() == QFileDevice::NoError,
          "Cannot read synthetic NAND region");
    return bytes;
  }

  void write_file(const QString& path, const QByteArray& bytes) {
    QFile file(path);
    check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly) && file.write(bytes) == bytes.size() && file.flush(),
          "Cannot create synthetic inventory fixture file");
  }

  void write_region(const QString& path, qint64 offset, const QByteArray& bytes) {
    QFile file(path);
    check(file.open(QIODevice::ReadWrite) && file.seek(offset) && file.write(bytes) == bytes.size() && file.flush(),
          "Cannot edit synthetic NAND fixture");
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot hash synthetic NAND fixture");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(hash.addData(&file), "Cannot complete synthetic NAND hash");
    return hash.result();
  }

  struct Invocation {
    int exit_code = -1;
    QByteArray output;
    QJsonObject report;
  };

  auto invoke(const QString& tool, const QString& image, const QString& output) -> Invocation {
    const QFileInfo prior_output(output);
    const bool occupied = prior_output.exists() || prior_output.isSymLink();
    QProcess process;
    process.setProcessChannelMode(QProcess::MergedChannels);
    process.start(tool, {"inventory", image, output});
    check(process.waitForFinished(120000), "Inventory command timed out");
    check(process.exitStatus() == QProcess::NormalExit, "Inventory command crashed");
    Invocation invocation;
    invocation.exit_code = process.exitCode();
    invocation.output = process.readAll();
    if (!occupied && QFileInfo(output).isFile()) {
      QJsonParseError error;
      const auto document = QJsonDocument::fromJson(read_file(output), &error);
      check(error.error == QJsonParseError::NoError && document.isObject(),
            "Inventory produced an invalid JSON report");
      invocation.report = document.object();
    }
    return invocation;
  }

  auto inspect_priiloader(const QString& tool, const QString& image) -> QJsonObject {
    QProcess process;
    process.setProcessChannelMode(QProcess::MergedChannels);
    process.start(tool, {"inspect-priiloader", image});
    check(process.waitForFinished(120000), "Priiloader inspection timed out");
    check(process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0,
          "Priiloader inspection did not pass");
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(process.readAll(), &error);
    check(error.error == QJsonParseError::NoError && document.isObject(),
          "Priiloader inspection produced invalid JSON");
    return document.object();
  }

  struct TmdRecord {
    quint32 cid;
    quint16 index;
    QByteArray content;
  };

  auto synthetic_menu_tmd(const QList<TmdRecord>& records, quint16 boot_index) -> QByteArray {
    QByteArray bytes(0x1e4 + records.size() * 0x24, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(bytes.data()));
    bytes.replace(0x140, 4, "Root");
    qToBigEndian<quint64>(0x0000000100000002ull, reinterpret_cast<uchar*>(bytes.data() + 0x18c));
    qToBigEndian<quint16>(513, reinterpret_cast<uchar*>(bytes.data() + 0x1dc));
    qToBigEndian<quint16>(static_cast<quint16>(records.size()),
                          reinterpret_cast<uchar*>(bytes.data() + 0x1de));
    qToBigEndian<quint16>(boot_index, reinterpret_cast<uchar*>(bytes.data() + 0x1e0));
    for (int i = 0; i < records.size(); ++i) {
      const auto& record = records.at(i);
      const int offset = 0x1e4 + i * 0x24;
      qToBigEndian<quint32>(record.cid, reinterpret_cast<uchar*>(bytes.data() + offset));
      qToBigEndian<quint16>(record.index, reinterpret_cast<uchar*>(bytes.data() + offset + 4));
      qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(bytes.data() + offset + 6));
      qToBigEndian<quint64>(static_cast<quint64>(record.content.size()),
                            reinterpret_cast<uchar*>(bytes.data() + offset + 8));
      const auto hash = GetSha1(record.content);
      std::copy(hash.cbegin(), hash.cend(), bytes.begin() + offset + 16);
    }
    return bytes;
  }

  void add_file(NandBin& nand, const QString& path, const QByteArray& bytes) {
    const auto id = nand.CreateEntry(path, 0, 0, NAND_FILE, NAND_RW, NAND_RW, 0);
    check(id && nand.SetData(id, bytes), "Cannot add synthetic Priiloader file");
  }

  auto success(const QString& tool, const QString& image, const QString& root, int& serial) -> QJsonObject {
    const auto report = QDir(root).filePath("inventory-" + QString::number(serial++) + ".json");
    const auto result = invoke(tool, image, report);
    check(result.exit_code == 0 && result.report.value("status").toString() == "pass",
          "Inventory did not pass for a valid NAND image");
    namespace fs = std::filesystem;
    check((fs::status(report.toStdString()).permissions() & fs::perms::mask) ==
              (fs::perms::owner_read | fs::perms::owner_write),
          "Inventory report is not owner-only");
    return result.report;
  }

  void replace_nand_file(const QString& image, const QString& path, const QByteArray& data) {
    NandBin nand;
    check(nand.SetPath(image) && nand.InitNand() && nand.SetData(path, data) && nand.WriteMetaData(),
          "Cannot mutate synthetic NAND map for parser test");
  }

  auto failure(const QString& tool, const QString& image, const QString& root, int& serial) -> QJsonObject {
    const auto report = QDir(root).filePath("invalid-" + QString::number(serial++) + ".json");
    const auto result = invoke(tool, image, report);
    check(result.exit_code != 0, "Inventory accepted a damaged NAND image");
    if (result.report.isEmpty()) return {};
    check(result.report.value("status").toString() == "fail", "Failed inventory has the wrong status");
    return result.report;
  }

  auto entry(const QJsonObject& report, const QString& path) -> QJsonObject {
    for (const auto value : report.value("entries").toArray()) {
      const auto item = value.toObject();
      if (item.value("path").toString() == path) return item;
    }
    throw std::runtime_error("Inventory omitted a required synthetic path");
  }

  auto file_entry_id(NandBin& nand, const QString& path) -> quint16 {
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    auto* current = tree->child(0);
    for (const auto& part : path.mid(1).split('/')) {
      QTreeWidgetItem* next = nullptr;
      for (int i = 0; current && i < current->childCount(); ++i)
        if (current->child(i)->text(0) == part) next = current->child(i);
      current = next;
    }
    check(current != nullptr, "Synthetic file is missing from NAND tree");
    bool ok = false;
    const auto id = current->text(1).toUShort(&ok);
    check(ok && id != 0, "Synthetic file has invalid FST index");
    return id;
  }

  auto raw_cluster_page(quint16 cluster) -> qint64 {
    return qint64(cluster) * 8 * 0x840;
  }

  auto metadata_offset(quint16 cluster, qint64 logical_offset) -> qint64 {
    return qint64(cluster) * 0x4200 + (logical_offset / 0x800) * 0x840 + logical_offset % 0x800;
  }
} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    test_priiloader_settings_writer();
    check(argc == 2, "Pass the built wii-nand executable");
    const auto tool = QFileInfo(app.arguments()[1]).absoluteFilePath();
    const auto scratch = QDir::current().filePath("private/tmp");
    check(QDir().mkpath(scratch), "Cannot create private inventory test scratch folder");
    QFile::setPermissions(scratch, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    QTemporaryDir temporary(QDir(scratch).filePath("inventory-tests-XXXXXX"));
    check(temporary.isValid(), "Cannot create private inventory test directory");
    QFile::setPermissions(temporary.path(), QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    const QDir root(temporary.path());
    const auto image = root.filePath("synthetic nand.bin");
    const auto keys = QByteArray::fromHex("0112233445566778899aabbccddeeff0").repeated(64);
    const QByteArray boot(0x108000, '\xff'); // Generic inventory intentionally does not require a boot signature.
    const auto file_path = QString("/shared2/data/sample.bin");
    const auto sample = QByteArray("a synthetic file payload; it must never appear in the JSON report\0", 66);
    const quint64 oversized_tid = 0x0001000101020304ull;
    QByteArray oversized_tmd(0x1e4 + 2 * 0x24, '\0');
    qToBigEndian<quint32>(ES_SIG_RSA2048, reinterpret_cast<uchar*>(oversized_tmd.data()));
    qToBigEndian<quint64>(oversized_tid, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x18c));
    qToBigEndian<quint16>(2, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x1de));
    qToBigEndian<quint32>(0x12345678, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x1e4));
    qToBigEndian<quint16>(7, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x1e8));
    qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x1ea));
    qToBigEndian<quint64>(0x20000001ull, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x1ec));
    qToBigEndian<quint32>(0x87654321, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x208));
    qToBigEndian<quint16>(8, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x20c));
    qToBigEndian<quint16>(1, reinterpret_cast<uchar*>(oversized_tmd.data() + 0x20e));
    qToBigEndian<quint64>(~quint64(0), reinterpret_cast<uchar*>(oversized_tmd.data() + 0x210));
    const auto oversized_title_path = QString("/title/00010001/01020304/content/title.tmd");
    {
      NandBin nand;
      check(nand.CreateNew(image, keys, boot, {42}), "Cannot create synthetic NAND image");
      for (const auto* directory : {"/sys", "/shared1", "/shared2", "/shared2/data", "/title",
                                    "/title/00010001", "/title/00010001/01020304",
                                    "/title/00010001/01020304/content"})
        check(nand.CreateEntry(directory, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0),
              "Cannot create synthetic NAND directory");
      UIDmap uid;
      uid.CreateNew();
      const auto uid_id = nand.CreateEntry("/sys/uid.sys", 0, 0, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(uid_id && nand.SetData(uid_id, uid.Data()), "Cannot write synthetic UID map");
      SharedContentMap shared;
      shared.AddEntry("00000000", QByteArray(20, 'a'));
      const auto shared_id = nand.CreateEntry("/shared1/content.map", 0, 0, NAND_FILE,
                                               NAND_READ, NAND_READ, NAND_READ);
      check(shared_id && nand.SetData(shared_id, shared.Data()), "Cannot write synthetic shared content map");
      const auto id = nand.CreateEntry(file_path, 0x1234, 2, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(id && nand.SetData(id, sample) && nand.WriteMetaData(), "Cannot write synthetic NAND file");
      const auto tmd_id = nand.CreateEntry(oversized_title_path, 0, 0, NAND_FILE,
                                           NAND_READ, NAND_READ, NAND_READ);
      check(tmd_id && nand.SetData(tmd_id, oversized_tmd) && nand.WriteMetaData(),
            "Cannot write synthetic title metadata");
    }

    const auto before = digest(image);
    check(QFile::setPermissions(image, QFileDevice::ReadOwner), "Cannot make NAND input read-only");
    int serial = 0;
    const auto report = success(tool, image, root.path(), serial);
    const auto readonly_report = QFileInfo(root.filePath("inventory-0.json"));
    check(readonly_report.isFile(), "Read-only inventory report was not written");
    check((readonly_report.permissions() & QFileDevice::ReadOwner) &&
              !(readonly_report.permissions() & (QFileDevice::WriteGroup | QFileDevice::WriteOther)),
          "Inventory report permissions are too broad");
    check((QFileInfo(image).permissions() & QFileDevice::WriteOwner) == 0,
          "Inventory made a read-only input writable");
    check(QFile::setPermissions(image, QFileDevice::ReadOwner | QFileDevice::WriteOwner),
          "Cannot restore synthetic NAND permissions");
    check(digest(image) == before, "Read-only inventory changed the NAND input");
    namespace fs = std::filesystem;
    check((fs::status(image.toStdString()).permissions() & fs::perms::mask) ==
              (fs::perms::owner_read | fs::perms::owner_write),
          "Inventory changed the NAND input permissions");

    check(report.value("schema_version").toInt() == 1 && report.value("kind").toString() == "nand-inventory",
          "Inventory report schema is missing or unsupported");
    const auto image_info = report.value("image").toObject();
    check(image_info.value("size").toInt() == 0x21000400 && image_info.value("key_source").toString() == "footer",
          "Inventory did not identify the synthetic NAND layout and key source");
    check(image_info.value("sha256").toString().size() == 64, "Inventory omitted its input hash");
    check(report.value("identity").toObject().value("keys_sha256").toString().size() == 64 &&
              report.value("identity").toObject().value("boot_sha256").toString().size() == 64,
          "Inventory omitted console input fingerprints");

    const auto missing_priiloader = inspect_priiloader(tool, image);
    check(missing_priiloader.value("status").toString() == "system-menu-unavailable" &&
              missing_priiloader.value("artifacts").toObject().isEmpty() &&
              missing_priiloader.value("settings").toObject().value("status").toString() == "missing",
          "Priiloader inspection misreported a NAND without a System Menu");

    const auto root_entry = entry(report, "/");
    check(root_entry.value("type").toString() == "directory", "Inventory omitted root directory facts");
    const auto data_entry = entry(report, file_path);
    check(data_entry.value("type").toString() == "file" && data_entry.value("size").toInt() == sample.size(),
          "Inventory omitted file type or size");
    check(data_entry.value("uid").toInt() == 0x1234 && data_entry.value("gid").toInt() == 2,
          "Inventory omitted file ownership");
    check(data_entry.value("sha256").toString() ==
              QString::fromLatin1(QCryptographicHash::hash(sample, QCryptographicHash::Sha256).toHex()),
          "Inventory file hash does not match decrypted file data");
    const auto maps = report.value("maps").toObject();
    const auto uid_map = maps.value("uid").toObject();
    check(uid_map.value("present").toBool() && uid_map.value("records").toArray().size() == 1 &&
              uid_map.value("records").toArray().at(0).toObject().value("title_id").toString() ==
                  "0000000100000002",
          "Inventory did not parse the synthetic UID map");
    const auto shared_map = maps.value("shared").toObject();
    check(shared_map.value("present").toBool() && shared_map.value("records").toArray().size() == 1 &&
              shared_map.value("records").toArray().at(0).toObject().value("name").toString() == "00000000",
          "Inventory did not parse the synthetic shared-content map");
    const auto titles = report.value("titles").toArray();
    check(titles.size() == 1 && titles.at(0).toObject().value("title_id").toString() == "0001000101020304" &&
              titles.at(0).toObject().value("content_count").toInt() == 2 &&
              titles.at(0).toObject().value("tmd_sha256").toString() ==
                  QString::fromLatin1(QCryptographicHash::hash(oversized_tmd, QCryptographicHash::Sha256).toHex()),
          "Inventory rejected or lost facts for a bounded TMD with a large declared content size");
    const auto report_text = QJsonDocument(report).toJson(QJsonDocument::Compact);
    check(!report_text.contains(sample), "Inventory disclosed NAND file contents");
    check(report.value("layout").toObject().value("bad_clusters").toArray().size() == 8,
          "Inventory omitted known synthetic bad clusters");

    const auto first_report = root.filePath("collision.json");
    write_file(first_report, "preserve this existing report");
    const auto collision = invoke(tool, image, first_report);
    check(collision.exit_code != 0 && read_file(first_report) == "preserve this existing report",
          "Inventory replaced an existing report");
    const auto link = root.filePath("report-link.json");
    std::filesystem::create_symlink(first_report.toStdString(), link.toStdString());
    const auto link_result = invoke(tool, image, link);
    check(link_result.exit_code != 0 && read_file(first_report) == "preserve this existing report",
          "Inventory followed a report symlink and changed its target");

    // Corrupt the root FST sibling pointer, which must be rejected by the raw
    // bounded-layout guard before WiiQt builds a recursive tree.
    const auto active = static_cast<quint16>(report.value("layout").toObject().value("active_metadata").toInt());
    check(active >= 0x7f00 && active <= 0x7ff0 && active % 16 == 0, "Inventory omitted active metadata location");
    const auto root_sibling = metadata_offset(active, 0x0c + 0x10000 + 0x10);
    const auto original_sibling = read_region(image, root_sibling, 2);
    check(original_sibling == QByteArray::fromHex("ffff"), "Synthetic root FST sibling has an unexpected value");
    write_region(image, root_sibling, QByteArray::fromHex("0000"));
    const auto malformed_tree = failure(tool, image, root.path(), serial);
    check(!malformed_tree.isEmpty(), "Malformed FST did not produce structured failure evidence");
    write_region(image, root_sibling, original_sibling);
    check(digest(image) == before, "FST rejection test did not restore the original NAND image");

    // A live data-page bit flip must fail integrity inspection, with no crash
    // and no payload copied into the diagnostics.
    quint16 cluster = 0;
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen synthetic NAND for corruption case");
      const auto file_id = file_entry_id(nand, file_path);
      const auto clusters = nand.GetFatsForFile(file_id);
      check(!clusters.isEmpty(), "Synthetic file has no data cluster");
      cluster = clusters.front();
    }
    const auto data_offset = raw_cluster_page(cluster);
    const auto byte = read_region(image, data_offset, 1);
    check(byte.size() == 1, "Cannot read synthetic data cluster");
    write_region(image, data_offset, QByteArray(1, static_cast<char>(byte[0] ^ 1)));
    const auto damaged_data = failure(tool, image, root.path(), serial);
    check(!damaged_data.isEmpty(), "Damaged data page did not produce structured failure evidence");
    QProcess damaged_priiloader;
    damaged_priiloader.start(tool, {"inspect-priiloader", image});
    check(damaged_priiloader.waitForFinished(120000) && damaged_priiloader.exitStatus() == QProcess::NormalExit &&
              damaged_priiloader.exitCode() != 0 && damaged_priiloader.readAllStandardOutput().isEmpty(),
          "Priiloader inspection produced evidence for unauthenticated data");
    write_region(image, data_offset, byte);
    check(digest(image) == before, "Data-page rejection test did not restore the original NAND image");

    // A matching sidecar is accepted, while disagreement with a NAND footer
    // key is rejected without changing either file.
    const auto sidecar = root.filePath("keys.bin");
    write_file(sidecar, keys);
    const auto matching = success(tool, image, root.path(), serial);
    check(matching.value("image").toObject().value("key_source").toString() == "footer-and-sidecar",
          "Inventory did not record its matching key sources");
    const auto saved_sidecar = read_file(sidecar);
    auto wrong_keys = saved_sidecar;
    wrong_keys[0] = static_cast<char>(wrong_keys[0] ^ 1);
    QFile::setPermissions(sidecar, QFileDevice::ReadOwner | QFileDevice::WriteOwner);
    write_region(sidecar, 0, wrong_keys);
    const auto mismatch = failure(tool, image, root.path(), serial);
    write_region(sidecar, 0, saved_sidecar);
    check(digest(sidecar) == QCryptographicHash::hash(saved_sidecar, QCryptographicHash::Sha256),
          "Key mismatch test did not restore its synthetic sidecar");
    check(digest(image) == before, "Inventory changed the NAND after a key mismatch");

    // Structural parsers must reject duplicate UID mappings and out-of-order
    // shared content IDs, while reporting no raw map bytes.
    UIDmap valid_uid;
    valid_uid.CreateNew();
    auto duplicate_uid = valid_uid.Data();
    duplicate_uid += valid_uid.Data();
    replace_nand_file(image, "/sys/uid.sys", duplicate_uid);
    const auto bad_uid_map = failure(tool, image, root.path(), serial);
    check(!bad_uid_map.isEmpty(), "Duplicate UID entries produced no failure report");
    replace_nand_file(image, "/sys/uid.sys", valid_uid.Data());

    SharedContentMap valid_shared;
    valid_shared.AddEntry("00000000", QByteArray(20, 'a'));
    SharedContentMap unordered_shared;
    unordered_shared.AddEntry("00000001", QByteArray(20, 'b'));
    replace_nand_file(image, "/shared1/content.map", unordered_shared.Data());
    const auto bad_shared_map = failure(tool, image, root.path(), serial);
    check(!bad_shared_map.isEmpty(), "Invalid shared-content ID produced no failure report");
    replace_nand_file(image, "/shared1/content.map", valid_shared.Data());

    // A title.tmd path with a truncated signed header is rejected structurally;
    // this inventory reports version facts but does not claim signature checks.
    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen synthetic NAND for TMD parser case");
      for (const auto* directory : {"/title/00000001", "/title/00000001/00000002",
                                     "/title/00000001/00000002/content"})
        check(nand.CreateEntry(directory, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0),
              "Cannot create synthetic title path");
      const auto tmd = nand.CreateEntry("/title/00000001/00000002/content/title.tmd", 0, 0,
                                        NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(tmd && nand.SetData(tmd, QByteArray(16, '\0')) && nand.WriteMetaData(),
            "Cannot create truncated synthetic TMD");
    }
    const auto malformed_tmd = failure(tool, image, root.path(), serial);
    check(!malformed_tmd.isEmpty(), "Truncated TMD produced no structured failure report");

    const QByteArray original_boot("original-system-menu-boot");
    const QByteArray active_boot("replacement-system-menu-boot");
    const QList<TmdRecord> original_records{
      {0x00000044, 7, QByteArray("other-content")},
      {0x00000025, 1, original_boot},
      {0x00000046, 0, QByteArray("second-content")},
    };
    auto active_records = original_records;
    active_records[1].content = active_boot;
    const auto original_menu_tmd = synthetic_menu_tmd(original_records, 1);
    const auto active_menu_tmd = synthetic_menu_tmd(active_records, 1);
    QByteArray loader_settings(32, '\0');
    loader_settings[0] = 3;
    loader_settings[8] = 1;
    loader_settings[20] = 80;
    loader_settings[21] = 1;
    loader_settings[27] = 2;
    qToBigEndian<quint32>(0x000a0000, reinterpret_cast<uchar*>(loader_settings.data() + 4));

    {
      NandBin nand;
      check(nand.SetPath(image) && nand.InitNand(), "Cannot reopen synthetic NAND for Priiloader inspection");
      check(nand.SetData("/title/00000001/00000002/content/title.tmd", active_menu_tmd),
            "Cannot write synthetic active System Menu TMD");
      add_file(nand, "/title/00000001/00000002/content/00000044.app", original_records[0].content);
      add_file(nand, "/title/00000001/00000002/content/00000025.app", active_boot);
      add_file(nand, "/title/00000001/00000002/content/00000046.app", original_records[2].content);
      add_file(nand, "/title/00000001/00000002/content/title_or.tmd", original_menu_tmd);
      add_file(nand, "/title/00000001/00000002/content/10000025.app", original_boot);
      check(nand.CreateEntry("/title/00000001/00000002/data", 0x1000, 1,
                             NAND_DIR, NAND_RW, 0, 0),
            "Cannot create synthetic Priiloader settings directory");
      add_file(nand, "/title/00000001/00000002/data/loader.ini", loader_settings);
      add_file(nand, "/title/00000001/00000002/data/hackshas.ini", QByteArray("private-state-marker"));
      check(nand.WriteMetaData(), "Cannot persist synthetic Priiloader evidence");
    }
    const auto priiloader_before = digest(image);
    const auto priiloader = inspect_priiloader(tool, image);
    check(priiloader.value("status").toString() == "unidentified-boot",
          "Unsigned synthetic TMD was treated as confirmed Priiloader");
    check(priiloader.value("image_sha256").toString() == QString::fromLatin1(priiloader_before.toHex()),
          "Priiloader inspection did not identify the inspected image");
    const auto menu = priiloader.value("system_menu").toObject();
    check(menu.value("tmd").toObject().value("version").toInt() == 513 &&
              menu.value("boot_app").toObject().value("path").toString().endsWith("/00000025.app") &&
              menu.value("boot_app").toObject().value("matches_tmd_hash").toBool() &&
              menu.value("boot_app").toObject().value("matches_tmd_size").toBool() &&
              menu.value("backup_app").toObject().value("path").toString().endsWith("/10000025.app") &&
              !menu.value("backup_app").toObject().value("matches_tmd_hash").toBool(true) &&
              menu.value("backup_app").toObject().value("matches_original_tmd_hash").toBool() &&
              !menu.value("backup_tmd").toObject().value("nintendo_signed").toBool(true),
          "Priiloader inspection did not follow TMD content indices or retain signature status");
    const auto artifacts = priiloader.value("artifacts").toObject();
    check(artifacts.value("loader.ini").toObject().value("size").toInt() == 32 &&
              priiloader.value("settings").toObject().value("status").toString() == "decoded" &&
              priiloader.value("settings").toObject().value("format").toString() == "0.10.0" &&
              artifacts.value("hackshas.ini").toObject().value("size").toInt() == 20,
          "Priiloader artifacts or pinned settings format were not reported");
    const auto values = priiloader.value("settings").toObject().value("values").toObject();
    check(values.value("autoboot").toString() == "system_menu" &&
              values.value("return_to").toString() == "priiloader" &&
              values.value("system_menu_ios").toInt() == 80 &&
              values.value("use_system_menu_ios").toBool() &&
              values.value("preferred_mount_point").toString() == "usb",
          "Priiloader settings offsets or values were decoded incorrectly");
    const auto priiloader_text = QJsonDocument(priiloader).toJson(QJsonDocument::Compact);
    check(!priiloader_text.contains(loader_settings) && !priiloader_text.contains("private-state-marker") &&
              !priiloader_text.contains(active_boot) && !priiloader_text.contains(original_boot),
          "Priiloader report disclosed private NAND file contents");
    check(digest(image) == priiloader_before,
          "Read-only Priiloader inspection changed the NAND image");

    const QString loader_path = "/title/00000001/00000002/data/loader.ini";
    auto invalid_settings = loader_settings;
    invalid_settings[22] = 2;
    replace_nand_file(image, loader_path, invalid_settings);
    const auto invalid = inspect_priiloader(tool, image).value("settings").toObject();
    check(invalid.value("status").toString() == "invalid" && !invalid.contains("values"),
          "Priiloader inspection decoded an invalid settings byte");
    replace_nand_file(image, loader_path, QByteArray(64, '\0'));
    const auto unsupported = inspect_priiloader(tool, image).value("settings").toObject();
    check(unsupported.value("status").toString() == "unsupported" && !unsupported.contains("values"),
          "Priiloader inspection decoded an unsupported settings layout");
    auto unknown_version = loader_settings;
    unknown_version[5] = 9;
    replace_nand_file(image, loader_path, unknown_version);
    const auto unknown = inspect_priiloader(tool, image).value("settings").toObject();
    check(unknown.value("status").toString() == "unsupported" && !unknown.contains("values"),
          "Priiloader inspection decoded an unrecognized settings version");

    std::cout << "Read-only NAND inventory checks passed.\n";
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
