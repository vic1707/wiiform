// SPDX-License-Identifier: GPL-2.0-only
#include <Seed.hpp>
#include <BootFlags.hpp>

#include <nandbin.h>
#include <blocks0to7.h>
#include <tools.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSet>
#include <QTemporaryDir>
#include <QtEndian>
#include <yaml-cpp/yaml.h>
#include <lzma.h>
#include <nandspare.h>

#include <filesystem>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  void write_new(const QString& path, const QByteArray& data) {
    QFile file(path);
    check(file.open(QIODevice::WriteOnly | QIODevice::NewOnly) && file.write(data) == data.size() && file.flush(),
          "Cannot write synthetic preflight fixture");
  }

  void write_region(const QString& path, qint64 offset, const QByteArray& bytes) {
    QFile file(path);
    check(file.open(QIODevice::ReadWrite) && file.seek(offset) && file.write(bytes) == bytes.size() && file.flush(),
          "Cannot edit synthetic NAND fixture");
  }

  void resize_file(const QString& path, qint64 size) {
    QFile file(path);
    check(file.open(QIODevice::ReadWrite) && file.resize(size), "Cannot resize synthetic NAND fixture");
  }

  auto read(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot read synthetic preflight fixture");
    const auto result = file.readAll();
    check(file.error() == QFileDevice::NoError, "Cannot finish synthetic preflight read");
    return result;
  }

  auto read_region(const QString& path, qint64 offset, qint64 size) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly) && file.seek(offset), "Cannot seek synthetic NAND fixture");
    const auto result = file.read(size);
    check(result.size() == size && file.error() == QFileDevice::NoError,
          "Cannot read synthetic NAND page");
    return result;
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot hash synthetic preflight input");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(hash.addData(&file), "Cannot finish synthetic preflight hash");
    return hash.result();
  }

  auto settings_entry(NandBin& nand) -> quint16 {
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    auto* current = tree->child(0);
    for (const auto& part : QString("title/00000001/00000002/data/setting.txt").split('/')) {
      QTreeWidgetItem* next = nullptr;
      for (int i = 0; current && i < current->childCount(); ++i)
        if (current->child(i)->text(0) == part) next = current->child(i);
      current = next;
    }
    check(current != nullptr, "Synthetic NAND omitted settings file");
    bool ok = false;
    const auto id = current->text(1).toUShort(&ok);
    check(ok && id != 0, "Synthetic settings entry has no id");
    return id;
  }

  auto active_metadata_cluster(const QJsonObject& report) -> quint16 {
    const auto layout = report.value("layout_summary").toObject().value("candidate").toObject();
    const auto cluster = layout.value("active_metadata").toInt(-1);
    check(cluster >= 0x7f00 && cluster <= 0x7ff0 && cluster % 16 == 0,
          "Preflight omitted its selected metadata cluster");
    return static_cast<quint16>(cluster);
  }

  auto physical_metadata_offset(quint16 active_metadata, qint64 logical_offset) -> qint64 {
    return qint64(active_metadata) * 0x4200 + (logical_offset / 0x800) * 0x840 + logical_offset % 0x800;
  }

  auto check_item(const QJsonObject& report, const char* id) -> QJsonObject {
    for (const auto value : report.value("checks").toArray()) {
      const auto item = value.toObject();
      if (item.value("id").toString() == id) return item;
    }
    throw std::runtime_error("Preflight omitted a required check");
  }

  struct Invocation {
    int exit_code = -1;
    QByteArray output;
    QJsonObject report;
  };

  auto invoke_worker(const QString& worker, const QString& seed, const QString& reference,
                     const QString& candidate, const QString& output, QStringList options = {}) -> Invocation {
    const QFileInfo prior_output(output);
    const bool destination_occupied = prior_output.exists() || prior_output.isSymLink();
    QProcess process;
    process.setProcessChannelMode(QProcess::MergedChannels);
    options.append(seed);
    if (!reference.isEmpty()) options.append(reference);
    options.append({candidate, output});
    process.start(worker, options);
    check(process.waitForFinished(120000), "Preflight worker timed out");
    check(process.exitStatus() == QProcess::NormalExit, "Preflight worker crashed");
    Invocation invocation;
    invocation.exit_code = process.exitCode();
    invocation.output = process.readAll();
    if (!destination_occupied && QFileInfo(output).isFile()) {
      QJsonParseError parse_error;
      const auto document = QJsonDocument::fromJson(read(output), &parse_error);
      check(parse_error.error == QJsonParseError::NoError && document.isObject(),
            "Preflight worker wrote invalid JSON");
      invocation.report = document.object();
      const auto status = invocation.report.value("host_status").toString();
      check((status == "pass" && invocation.exit_code == 0) ||
                (status == "fail" && invocation.exit_code == 1) ||
                (status == "blocked" && invocation.exit_code == 3),
            "Preflight worker exit code differs from its report");
    }
    return invocation;
  }

  void expect_status(const QJsonObject& report, const char* id, const char* status) {
    const auto item = check_item(report, id);
    if (item.value("status").toString() != status)
      throw std::runtime_error(std::string("Preflight ") + id + " expected " + status + ": " +
                               item.value("status").toString().toStdString() + "; " +
                               item.value("detail").toString().toStdString());
  }

  auto crypt_settings(QByteArray data) -> QByteArray {
    quint32 key = 0x73b5dbfa;
    for (int i = 0; i < data.size(); ++i) {
      data[i] = data[i] ^ static_cast<char>(key & 0xff);
      key = (key << 1) | (key >> 31);
    }
    data.append(QByteArray(256 - data.size(), '\0'));
    return data;
  }

  auto decrypt_settings(QByteArray data) -> QByteArray {
    quint32 key = 0x73b5dbfa;
    for (int i = 0; i < data.size(); ++i) {
      data[i] = data[i] ^ static_cast<char>(key & 0xff);
      key = (key << 1) | (key >> 31);
    }
    return data;
  }

  auto settings_hmac_offset(const QString& path) -> qint64 {
    quint16 file_id = 0;
    QList<quint16> clusters;
    {
      NandBin nand;
      check(nand.SetPath(path) && nand.InitNand(), "Cannot index synthetic NAND settings");
      file_id = settings_entry(nand);
      check(nand.CheckHmacData(file_id), "Synthetic settings HMAC is invalid");
      clusters = nand.GetFatsForFile(file_id);
    }
    check(!clusters.isEmpty(), "Synthetic settings have no allocated cluster");
    return qint64(clusters.front()) * 8 * 0x840 + 6 * 0x840;
  }

  auto write_seed(const QString& path, const YAML::Node& manifest) -> void {
    write_new(path, QByteArray::fromStdString(YAML::Dump(manifest)));
  }

  auto write_boot_seed(const seed::Components& source, const QByteArray& boots, const QString& path) -> void {
    auto manifest = YAML::Clone(source.manifest);
    check(boots.size() == 0x108000, "Synthetic boot fixture has an unexpected size");
    QByteArray compressed(static_cast<int>(lzma_stream_buffer_bound(static_cast<size_t>(boots.size()))), '\0');
    size_t output_pos = 0;
    const auto result = lzma_easy_buffer_encode(
        6, LZMA_CHECK_CRC64, nullptr, reinterpret_cast<const uint8_t*>(boots.constData()),
        static_cast<size_t>(boots.size()), reinterpret_cast<uint8_t*>(compressed.data()), &output_pos,
        static_cast<size_t>(compressed.size()));
    check(result == LZMA_OK, "Cannot compress altered synthetic boot bytes");
    compressed.resize(static_cast<int>(output_pos));
    manifest["boot_blocks"]["_sha256"] =
        QCryptographicHash::hash(boots, QCryptographicHash::Sha256).toHex().toStdString();
    manifest["boot_blocks"]["xz"] = YAML::Binary(reinterpret_cast<const unsigned char*>(compressed.constData()),
                                                  static_cast<std::size_t>(compressed.size()));
    manifest["boot_blocks"]["xz"].SetTag("tag:yaml.org,2002:binary");
    write_seed(path, manifest);
  }

  auto mutate_boot_seed(const seed::Components& source, const QString& path) -> void {
    auto boots = source.files.value("boot-blocks.bin");
    check(!boots.isEmpty(), "Seed decoder omitted boot bytes");
    boots[0] = static_cast<char>(boots[0] ^ 1);
    write_boot_seed(source, boots, path);
  }

  auto add_map_lookalike(QByteArray& boots, int block, quint32 generation) -> void {
    check(block >= 1 && block < 8, "Synthetic map block is outside the boot area");
    QByteArray record = QByteArray::fromHex("26f29a401ee684cf");
    for (int i = 0; i < 4; ++i) record.append(static_cast<char>((generation >> (8 * i)) & 0xff));
    record.append(QByteArray(8, '\0'));
    record.append(QByteArray(0x4c - record.size(), '\0'));
    const auto page = block * 64 + 63;
    const auto offset = page * 0x840;
    for (int copy = 0; copy < 3; ++copy)
      boots.replace(offset + copy * record.size(), record.size(), record);
  }

  auto mutate_settings_seed(const seed::Components& source, const QString& path) -> void {
    auto manifest = YAML::Clone(source.manifest);
    auto plain = decrypt_settings(source.files.value("setting.txt"));
    check(plain.contains("SERNO=SECRET1234"), "Synthetic settings marker was not found");
    plain.replace("SERNO=SECRET1234", "SERNO=ALTNUM1234");
    const auto settings = crypt_settings(plain);
    manifest["settings"]["fields"]["SERNO"] = "ALTNUM1234";
    manifest["settings"]["_sha256"] =
        QCryptographicHash::hash(settings, QCryptographicHash::Sha256).toHex().toStdString();
    write_seed(path, manifest);
  }
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    check(!boot::has_bad_content_hash(BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK),
          "Valid boot2 signature flags were rejected");
    check(boot::has_bad_content_hash(BOOT_2_BAD_CONTENT_HASH | BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK),
          "Composite BAD_CONTENT_HASH flag was ignored");
    const auto parent = QDir::current().filePath("build");
    QTemporaryDir temporary(QDir(parent).filePath("preflight-tests-XXXXXX"));
    check(temporary.isValid(), "Cannot create preflight scratch folder in the checkout");
    const QDir root(temporary.path());

    const auto reference = root.filePath("reference.bin");
    const auto seed_path = root.filePath("console.seed.UNENCRYPTED.yaml");
    const auto report_path = root.filePath("preflight.json");
    const auto worker = QDir::cleanPath(QCoreApplication::applicationDirPath() + "/../libexec/wiiform-readonly");
    int report_index = 0;
    auto inspect = [&](const QString& seed, const QString& reference_path, const QString& candidate_path) {
      const auto output = root.filePath("report-" + QString::number(report_index++) + ".json");
      const auto invocation = invoke_worker(worker, seed, reference_path, candidate_path, output);
      check(!invocation.report.isEmpty(), "Preflight worker produced no report");
      return invocation.report;
    };
    QByteArray keys(0x400, '\0');
    for (int i = 0; i < keys.size(); ++i) keys[i] = static_cast<char>((i * 37 + 11) & 0xff);
    QByteArray boots(0x108000, '\xff');
    // Deliberately unrecognized synthetic boot bytes must fail the production boot gate.
    for (int i = 0; i < 0x800; ++i) boots[i] = static_cast<char>((i * 13 + 91) & 0xff);
    const auto settings = crypt_settings("AREA=EUR\r\nMODEL=RVL-001(EUR)\r\nDVD=0\r\nMPCH=0x7FFE\r\n"
                                         "CODE=TEST\r\nSERNO=SECRET1234\r\nVIDEO=PAL\r\nGAME=EU\r\n");
    const QString setting_path = "/title/00000001/00000002/data/setting.txt";
    {
      NandBin nand;
      check(nand.CreateNew(reference, keys, boots, {42}), "Cannot create synthetic reference NAND");
      for (const auto* path : {"/title", "/title/00000001", "/title/00000001/00000002",
                               "/title/00000001/00000002/data"})
        check(nand.CreateEntry(path, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0), "Cannot create synthetic NAND path");
      const auto entry = nand.CreateEntry(setting_path, 0x1000, 1, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(entry && nand.SetData(entry, settings) && nand.WriteMetaData(), "Cannot write synthetic settings");
    }
    seed::export_backup(reference, seed_path);
    const auto decoded = seed::read(seed_path);
    check(decoded.files.value("keys.bin") == keys && decoded.files.value("boot-blocks.bin") == boots &&
              decoded.files.value("setting.txt") == settings && decoded.files.value("bad-blocks.txt") == "42\n",
          "In-memory seed decode changed reconstructed components");

    const auto reference_hash = digest(reference);
    const auto seed_hash = digest(seed_path);
    check(QFile::setPermissions(reference, QFileDevice::ReadOwner) &&
              QFile::setPermissions(seed_path, QFileDevice::ReadOwner),
          "Cannot make synthetic NAND and seed read-only");
    auto report = inspect(seed_path, reference, reference);
    check(QFile::setPermissions(reference, QFileDevice::ReadOwner | QFileDevice::WriteOwner) &&
              QFile::setPermissions(seed_path, QFileDevice::ReadOwner | QFileDevice::WriteOwner),
          "Cannot restore synthetic fixture permissions");
    check(report.value("restore_status").toString() == "blocked", "Preflight must always block restore eligibility");
    expect_status(report, "seed", "pass");
    expect_status(report, "reference.input", "pass");
    expect_status(report, "reference.authentication", "pass");
    expect_status(report, "reference.identity", "pass");
    expect_status(report, "reference.settings", "pass");
    expect_status(report, "reference.boot", "pass");
    expect_status(report, "reference.bad_blocks", "pass");
    expect_status(report, "reference.layout", "pass");
    expect_status(report, "candidate.input", "pass");
    expect_status(report, "candidate.layout", "pass");
    expect_status(report, "candidate.identity", "pass");
    expect_status(report, "candidate.authentication", "pass");
    expect_status(report, "candidate.settings", "pass");
    expect_status(report, "candidate.boot", "pass");
    expect_status(report, "candidate.boot_signature", "fail");
    expect_status(report, "candidate.filesystem", "pass");
    expect_status(report, "candidate.metadata", "fail");
    expect_status(report, "candidate.stock", "fail");
    const auto active_metadata = active_metadata_cluster(report);
    expect_status(report, "inputs.unchanged", "pass");
    expect_status(report, "recovery", "blocked");
    expect_status(report, "restore_boundary", "blocked");
    expect_status(report, "hardware", "blocked");
    check(digest(reference) == reference_hash && digest(seed_path) == seed_hash,
          "Preflight changed its NAND or seed input");

    const auto override_report = invoke_worker(worker, seed_path, reference, reference,
        root.filePath("override.json"), {"--setting-override", "AREA=USA", "--setting-override", "GAME=US",
                                         "--setting-override", "VIDEO=NTSC"}).report;
    expect_status(override_report, "reference.settings", "pass");
    expect_status(override_report, "candidate.settings", "fail");
    check(override_report.value("region").toString() == "USA", "Settings override did not select the USA profile");
    const auto candidate_only = invoke_worker(worker, seed_path, {}, reference,
        root.filePath("candidate-only.json"), {"--candidate-only"}).report;
    expect_status(candidate_only, "candidate.settings", "pass");
    check(!candidate_only.value("reference_requested").toBool(true) &&
              !candidate_only.value("input_sha256").toObject().contains("reference"),
          "Candidate-only report claims a reference check");

    // Map-shaped bytes at the historical pages are not enough to clear an ECC
    // warning. Keep the boot area identical to its seed while both boot1 and
    // boot2 remain synthetic and unrecognized; the production signature gate
    // must leave these all-FF ECC mismatches unclassified and blocked.
    auto map_lookalike_boots = boots;
    add_map_lookalike(map_lookalike_boots, 2, 1);
    add_map_lookalike(map_lookalike_boots, 6, 2);
    for (const auto page : {191, 447})
      check(map_lookalike_boots.mid(page * 0x840 + 0x830, 0x10) == QByteArray(0x10, '\xff'),
            "Synthetic map-like page unexpectedly has written ECC");
    const auto map_seed = root.filePath("map-lookalike.seed.yaml");
    write_boot_seed(decoded, map_lookalike_boots, map_seed);
    write_region(reference, 0, map_lookalike_boots);
    const auto map_lookalike_report = inspect(map_seed, reference, reference);
    write_region(reference, 0, boots);
    check(digest(reference) == reference_hash, "Map-like ECC regression did not restore its NAND fixture");
    expect_status(map_lookalike_report, "candidate.boot", "pass");
    expect_status(map_lookalike_report, "candidate.boot_signature", "fail");
    expect_status(map_lookalike_report, "candidate.boot_ecc", "blocked");
    QSet<int> unclassified_pages;
    for (const auto value : map_lookalike_report.value("boot_ecc").toArray()) {
      const auto diagnostic = value.toObject();
      if (diagnostic.value("page").toInt() == 191 || diagnostic.value("page").toInt() == 447) {
        check(diagnostic.value("type").toString() == "unclassified" &&
                  diagnostic.value("ecc_unwritten").toBool() && diagnostic.value("matches_seed").toBool(),
              "Map-shaped ECC page passed without the stock boot gate");
        unclassified_pages.insert(diagnostic.value("page").toInt());
      }
    }
    check(unclassified_pages.size() == 2 && unclassified_pages.contains(191) && unclassified_pages.contains(447),
          "Synthetic map-shaped ECC pages were omitted from diagnostics");

    const auto private_json = QJsonDocument(report).toJson(QJsonDocument::Compact);
    check(!private_json.contains("SECRET1234") && !private_json.contains(keys.toHex()),
          "Preflight report disclosed settings or key bytes");

    const auto write_invocation = invoke_worker(worker, seed_path, reference, reference, report_path);
    check((write_invocation.exit_code == 0 || write_invocation.exit_code == 1 || write_invocation.exit_code == 3) &&
              !write_invocation.report.isEmpty(),
          "Preflight worker failed while writing a new report");
    check(QFileInfo(report_path).exists(), "Preflight did not create its report");
    namespace fs = std::filesystem;
    check((fs::status(report_path.toStdString()).permissions() & fs::perms::mask) ==
              (fs::perms::owner_read | fs::perms::owner_write),
          "Preflight report is not owner-only");
    const auto report_bytes = read(report_path);
    const auto collision = invoke_worker(worker, seed_path, reference, reference, report_path);
    check(collision.exit_code != 0 && collision.report.isEmpty(), "Preflight replaced an existing report");
    check(read(report_path) == report_bytes, "Existing report changed after collision");

    const auto bad_yaml = root.filePath("malformed.seed.yaml");
    write_new(bad_yaml, "keys: SECRET1234: [\n");
    bool generic_error = false;
    try {
      static_cast<void>(seed::read(bad_yaml));
    } catch (const std::exception& error) {
      generic_error = std::string(error.what()).find("SECRET1234") == std::string::npos;
    }
    check(generic_error, "Malformed seed diagnostics exposed input text");
    const auto malformed_report = inspect(bad_yaml, reference, reference);
    expect_status(malformed_report, "seed", "fail");
    check(!QJsonDocument(malformed_report).toJson(QJsonDocument::Compact).contains("SECRET1234"),
          "Malformed seed report exposed source text");

    const auto changed_boot_seed = root.filePath("changed-boot.seed.yaml");
    mutate_boot_seed(decoded, changed_boot_seed);
    const auto changed_boot = inspect(changed_boot_seed, reference, reference);
    expect_status(changed_boot, "seed", "pass");
    expect_status(changed_boot, "candidate.boot", "fail");
    expect_status(changed_boot, "reference.boot", "blocked");
    check(changed_boot.value("restore_status").toString() == "blocked",
          "Altered seed boot bytes changed restore status");

    const auto changed_settings_seed = root.filePath("changed-settings.seed.yaml");
    mutate_settings_seed(decoded, changed_settings_seed);
    const auto changed_settings = inspect(changed_settings_seed, reference, reference);
    expect_status(changed_settings, "seed", "pass");
    expect_status(changed_settings, "reference.identity", "pass");
    expect_status(changed_settings, "reference.settings", "fail");
    expect_status(changed_settings, "candidate.settings", "fail");

    const auto sidecar_path = root.filePath("keys.bin");
    resize_file(reference, 0x21000000);
    write_new(sidecar_path, keys);
    const auto sidecar_report = inspect(seed_path, reference, reference);
    expect_status(sidecar_report, "reference.input", "pass");
    expect_status(sidecar_report, "reference.identity", "pass");
    expect_status(sidecar_report, "reference.authentication", "pass");
    expect_status(sidecar_report, "reference.layout", "pass");
    expect_status(sidecar_report, "candidate.identity", "pass");
    expect_status(sidecar_report, "candidate.authentication", "pass");
    expect_status(sidecar_report, "candidate.layout", "pass");
    check(QFile::remove(sidecar_path), "Cannot remove matching synthetic sidecar");
    resize_file(reference, 0x21000400);
    write_region(reference, 0x21000000, keys);
    check(digest(reference) == reference_hash, "Sidecar-only inspection did not restore its synthetic fixture");

    const auto wrong_sidecar = sidecar_path;
    auto mismatched_keys = keys;
    mismatched_keys[0] = static_cast<char>(mismatched_keys[0] ^ 1);
    write_new(wrong_sidecar, mismatched_keys);
    const auto ambiguous_keys = inspect(seed_path, reference, reference);
    expect_status(ambiguous_keys, "reference.input", "fail");
    expect_status(ambiguous_keys, "candidate.input", "fail");
    check(QFile::remove(wrong_sidecar), "Cannot remove synthetic mismatched sidecar");

    auto altered_footer = keys;
    altered_footer[1] = static_cast<char>(altered_footer[1] ^ 1);
    write_region(reference, 0x21000000, altered_footer);
    const auto wrong_footer = inspect(seed_path, reference, reference);
    expect_status(wrong_footer, "reference.input", "pass");
    expect_status(wrong_footer, "reference.identity", "fail");
    expect_status(wrong_footer, "candidate.identity", "fail");
    write_region(reference, 0x21000000, keys);

    quint16 settings_id = 0;
    QList<quint16> settings_clusters;
    {
      NandBin nand;
      check(nand.SetPath(reference) && nand.InitNand(), "Cannot locate synthetic layout entries");
      settings_id = settings_entry(nand);
      settings_clusters = nand.GetFatsForFile(settings_id);
    }
    check(!settings_clusters.isEmpty(), "Synthetic settings entry has no FAT cluster");

    const qint64 sibling_logical = 0x1000c + qint64(settings_id) * 0x20 + 0x10;
    const auto sibling_physical = physical_metadata_offset(active_metadata, sibling_logical);
    const auto original_sibling = read_region(reference, sibling_physical, 2);
    QByteArray cyclic_sibling(2, '\0');
    qToBigEndian<quint16>(settings_id, reinterpret_cast<uchar*>(cyclic_sibling.data()));
    write_region(reference, sibling_physical, cyclic_sibling);
    const auto sibling_cycle = inspect(seed_path, reference, reference);
    expect_status(sibling_cycle, "candidate.layout", "fail");
    write_region(reference, sibling_physical, original_sibling);

    const auto data_cluster = settings_clusters.front();
    const qint64 fat_logical = 0x0c + qint64(data_cluster) * 2;
    const auto fat_physical = physical_metadata_offset(active_metadata, fat_logical);
    const auto original_fat = read_region(reference, fat_physical, 2);
    QByteArray self_cycle(2, '\0');
    qToBigEndian<quint16>(data_cluster, reinterpret_cast<uchar*>(self_cycle.data()));
    write_region(reference, fat_physical, self_cycle);
    const auto fat_cycle = inspect(seed_path, reference, reference);
    expect_status(fat_cycle, "candidate.layout", "fail");
    write_region(reference, fat_physical, original_fat);

    const auto orphan_physical = physical_metadata_offset(active_metadata, 0x1000c + 0x17fe * 0x20);
    const auto original_orphan = read_region(reference, orphan_physical, 0x20);
    check(original_orphan == QByteArray(0x20, '\0'), "Synthetic orphan slot was already used");
    QByteArray orphan(0x20, '\0');
    orphan.replace(0, 6, "orphan");
    orphan[0x0c] = NAND_FILE;
    qToBigEndian<quint16>(0xfffb, reinterpret_cast<uchar*>(orphan.data() + 0x0e));
    qToBigEndian<quint16>(0xffff, reinterpret_cast<uchar*>(orphan.data() + 0x10));
    write_region(reference, orphan_physical, orphan);
    const auto orphan_report = inspect(seed_path, reference, reference);
    expect_status(orphan_report, "candidate.layout", "fail");
    expect_status(orphan_report, "reference.layout", "pass");
    expect_status(orphan_report, "reference.allocation", "warn");
    check(orphan_report["layout_summary"].toObject()["reference"].toObject()["orphan_entries"].toInt() == 1,
          "Historical orphan FST entry was not recorded");
    write_region(reference, orphan_physical, original_orphan);

    const qint64 metadata_page_offset = qint64(0x7f00) * 8 * 0x840;
    const auto original_metadata_page = read_region(reference, metadata_page_offset, 0x840);
    auto bad_ecc_page = original_metadata_page;
    bad_ecc_page[0x830] = static_cast<char>(bad_ecc_page[0x830] ^ 1);
    write_region(reference, metadata_page_offset, bad_ecc_page);
    const auto bad_ecc = inspect(seed_path, reference, reference);
    expect_status(bad_ecc, "candidate.filesystem", "fail");
    write_region(reference, metadata_page_offset, original_metadata_page);

    auto bad_hmac_page = original_metadata_page;
    bad_hmac_page[0] = static_cast<char>(bad_hmac_page[0] ^ 1);
    NandSpare spare;
    bad_hmac_page.replace(0x830, 0x10, spare.CalcEcc(bad_hmac_page.left(0x800)));
    write_region(reference, metadata_page_offset, bad_hmac_page);
    const auto bad_hmac = inspect(seed_path, reference, reference);
    expect_status(bad_hmac, "candidate.filesystem", "fail");
    write_region(reference, metadata_page_offset, original_metadata_page);

    const auto file_hmac_offset = settings_hmac_offset(reference);
    const auto original_file_hmac_page = read_region(reference, file_hmac_offset, 0x840);
    auto bad_file_hmac_page = original_file_hmac_page;
    bad_file_hmac_page[0x801] = static_cast<char>(bad_file_hmac_page[0x801] ^ 1);
    write_region(reference, file_hmac_offset, bad_file_hmac_page);
    const auto bad_file_hmac = inspect(seed_path, reference, reference);
    expect_status(bad_file_hmac, "candidate.filesystem", "fail");
    write_region(reference, file_hmac_offset, original_file_hmac_page);
    check(digest(reference) == reference_hash, "Synthetic NAND mutations were not fully restored");

    auto changed_badblocks_manifest = YAML::Clone(decoded.manifest);
    changed_badblocks_manifest["bad_blocks"].push_back(43);
    const auto changed_badblocks_seed = root.filePath("changed-bad-blocks.seed.yaml");
    write_seed(changed_badblocks_seed, changed_badblocks_manifest);
    const auto changed_badblocks = inspect(changed_badblocks_seed, reference, reference);
    expect_status(changed_badblocks, "candidate.bad_blocks", "fail");
    check(changed_badblocks.value("restore_status").toString() == "blocked",
          "Bad-block mismatch changed restore status");

    const auto missing = inspect(root.filePath("missing.seed.yaml"), reference, reference);
    expect_status(missing, "seed", "fail");
    check(missing.value("restore_status").toString() == "blocked",
          "Missing input changed restore status");

    const auto missing_report = root.filePath("missing-output-parent/report.json");
    const auto parent_rejection = invoke_worker(worker, seed_path, reference, reference, missing_report);
    check(parent_rejection.exit_code != 0 && parent_rejection.report.isEmpty(),
          "Preflight created a missing output parent implicitly");

    const auto dangling_report_link = root.filePath("dangling-report.json");
    check(QFile::link(root.filePath("absent-report.json"), dangling_report_link),
          "Cannot create dangling report symlink");
    const auto link_rejection = invoke_worker(worker, seed_path, reference, reference, dangling_report_link);
    check(link_rejection.exit_code != 0 && !QFileInfo(dangling_report_link).exists(),
          "Preflight followed a dangling report symlink");

    const auto frontend_root = root.filePath("frontend");
    const auto frontend_bin = QDir(frontend_root).filePath("bin");
    check(QDir().mkpath(frontend_bin), "Cannot create isolated frontend folder");
    const auto frontend = QDir(frontend_bin).filePath("wii-nand");
    const auto built_frontend = QDir(QCoreApplication::applicationDirPath()).filePath("wii-nand");
    check(QFile::copy(built_frontend, frontend), "Cannot copy the built frontend into synthetic scratch");
    check(QFile::setPermissions(frontend, QFileInfo(built_frontend).permissions()),
          "Cannot preserve the frontend's executable permissions");
    QProcess missing_worker;
    missing_worker.setProcessChannelMode(QProcess::MergedChannels);
    missing_worker.start(frontend, {"preflight", seed_path, reference, reference,
                                    root.filePath("missing-worker-report.json")});
    check(missing_worker.waitForFinished(120000) && missing_worker.exitStatus() == QProcess::NormalExit &&
              missing_worker.exitCode() != 0 &&
              missing_worker.readAll().contains("worker is missing") &&
              !QFileInfo::exists(root.filePath("missing-worker-report.json")),
          "Public frontend did not handle a missing readonly worker safely");

    std::cout << "Preflight synthetic identity, boot rejection, redaction and output checks passed.\n";
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
