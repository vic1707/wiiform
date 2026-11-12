// SPDX-License-Identifier: GPL-2.0-only
#include <nandbin.h>
#include <tools.h>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QTemporaryDir>
#include <yaml-cpp/yaml.h>

#include <filesystem>
#include <functional>
#include <iostream>
#include <random>
#include <stdexcept>

namespace {
  void check(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto read(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot read test output");
    const auto data = file.readAll();
    check(file.error() == QFileDevice::NoError, "Incomplete test read");
    return data;
  }

  void write(const QString& path, const QByteArray& data) {
    QFile file(path);
    check(file.open(QIODevice::WriteOnly) && file.write(data) == data.size(), "Cannot write test data");
  }

  auto digest(const QString& path) -> QByteArray {
    QFile file(path);
    check(file.open(QIODevice::ReadOnly), "Cannot hash test input");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    check(hash.addData(&file), "Cannot finish test hash");
    return hash.result();
  }

  auto crypt(QByteArray data) -> QByteArray {
    quint32 key = 0x73b5dbfa;
    for (int i = 0; i < data.size(); ++i) {
      data[i] = data[i] ^ static_cast<char>(key & 0xff);
      key = (key << 1) | (key >> 31);
    }
    data.append(QByteArray(256 - data.size(), '\0'));
    return data;
  }
} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  try {
    check(argc == 2 || argc == 3, "Pass the executable and optionally a new synthetic seed filename");
    const auto tool = QFileInfo(app.arguments()[1]).absoluteFilePath();
    QTemporaryDir temporary;
    check(temporary.isValid(), "Cannot create private test directory");
    const QDir root(temporary.path());
    const auto backup = root.filePath("nand.bin");
    const auto seed = root.filePath("console.seed.yaml");
    const auto output = root.filePath("components");
    std::mt19937 random(12345);
    QByteArray keys(0x400, '\0');
    for (auto& byte : keys) byte = static_cast<char>(random() & 0xff);
    keys[3] = '\0';
    keys[7] = '\x7f';
    keys.replace(0x124, 4, QByteArray::fromHex("00000001"));
    QByteArray boots(0x108000, '\xff');
    for (int i = 0; i < 0x800; ++i) boots[i] = static_cast<char>(random() & 0xff);
    const auto settings = crypt("AREA=EUR\r\nMODEL=RVL-001(EUR)\r\nDVD=0\r\nMPCH=0x7FFE\r\n"
                                "CODE=TEST\r\nSERNO=001234567\r\nVIDEO=PAL\r\nGAME=EU\r\n");
    const QString setting_path = "/title/00000001/00000002/data/setting.txt";
    {
      NandBin nand;
      check(!QFileInfo::exists(backup) && nand.CreateNew(backup, keys, boots, {42}), "Create fixture failed");
      for (const auto* path :
           {"/title", "/title/00000001", "/title/00000001/00000002", "/title/00000001/00000002/data"})
        check(nand.CreateEntry(path, 0, 0, NAND_DIR, NAND_RW, NAND_RW, 0), "Create fixture directory failed");
      const auto entry = nand.CreateEntry(setting_path, 0x1000, 1, NAND_FILE, NAND_READ, NAND_READ, NAND_READ);
      check(entry && nand.SetData(entry, settings) && nand.WriteMetaData(), "Write fixture settings failed");
    }
    const auto original_hash = digest(backup);
    auto run = [&](const QStringList& arguments, bool success = true) {
      QProcess process;
      process.setProcessChannelMode(QProcess::MergedChannels);
      process.start(tool, arguments);
      check(process.waitForFinished(120000), "CLI timed out");
      const auto passed = process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0;
      if (passed != success) {
        std::cerr << process.readAll().toStdString();
        throw std::runtime_error("Unexpected CLI status for " + arguments.join(' ').toStdString() +
                                 (success ? " success" : " rejection"));
      }
    };
    run({"export", backup, seed});
    run({"unpack", seed, output});
    check(digest(backup) == original_hash, "Export changed the source backup");
    check(read(output + "/keys.bin") == keys, "Keys changed (unknown bytes, overlap or embedded NUL)");
    check(read(output + "/boot-blocks.bin") == boots, "Boot bytes changed");
    check(read(output + "/setting.txt") == settings, "Canonical settings did not reconstruct exactly");
    check(read(output + "/bad-blocks.txt") == "42\n", "Bad-block list changed");
    const auto original = read(seed);
    check(!original.contains('\x7f'), "YAML contains an unescaped DEL control character");
    check(original.indexOf("\nsettings:") < original.indexOf("\nboot_blocks:"),
          "Opaque boot data must follow the readable settings and metadata");
    const auto manifest = YAML::Load(original.toStdString());
    check(!manifest["format_version"] && !manifest["boot_blocks"]["bytes"],
          "New seeds should omit schema flags and redundant boot size");
    check(manifest["bootmii_boot2"].as<std::string>() == "unknown",
          "Unrecognized synthetic boot area was classified as stock or BootMii");
    for (const auto& copy : manifest["boot2"])
      check(!copy["flags"], "New seeds should not duplicate the raw boot2 state flags");
    check(manifest["boot_blocks"]["xz"].Tag() == "tag:yaml.org,2002:binary", "Boot data is not YAML binary");
    check(!manifest["settings"]["original"], "Canonical settings unnecessarily retained a binary copy");
    check(manifest["keys"]["otp"]["console_id"].Tag() == "!" &&
              manifest["settings"]["fields"]["SERNO"].Tag() == "!" &&
              manifest["settings"]["fields"]["DVD"].Tag() == "!" &&
              manifest["settings"]["fields"]["SERNO"].as<std::string>() == "001234567",
          "String fields must be quoted to prevent SOPS type coercion");
    check(original.contains("!!binary") && !original.contains("!<tag:yaml.org,2002:"),
          "Seed output uses verbose YAML tags");
    check(manifest["keys"]["extra"].size() > 0, "Fixture does not cover unknown key bytes");
    check(manifest["source"]["sha256"].IsScalar() && manifest["keys"]["_sha256"].IsScalar() &&
              manifest["boot_blocks"]["_sha256"].IsScalar() && manifest["settings"]["_sha256"].IsScalar(),
          "Hashes must be grouped with their payload");

    // Historical version metadata may remain after YAML tooling edits a seed.
    auto prior_current = YAML::Clone(manifest);
    prior_current["format_version"] = 3;
    const auto prior_seed = root.filePath("prior-current.seed.yaml");
    const auto prior_output = root.filePath("prior-current-output");
    write(prior_seed, QByteArray::fromStdString(YAML::Dump(prior_current)));
    run({"unpack", prior_seed, prior_output});
    check(read(prior_output + "/keys.bin") == keys && read(prior_output + "/boot-blocks.bin") == boots &&
              read(prior_output + "/setting.txt") == settings,
          "Historical version metadata changed current-format seed payloads");
    namespace fs = std::filesystem;
    check((fs::status(seed.toStdString()).permissions() & fs::perms::mask) ==
              (fs::perms::owner_read | fs::perms::owner_write),
          "Seed is not 0600");
    check((fs::status(output.toStdString()).permissions() & fs::perms::mask) == fs::perms::owner_all,
          "Output directory is not 0700");
    for (const auto& file : fs::directory_iterator(output.toStdString()))
      check((file.status().permissions() & fs::perms::mask) == (fs::perms::owner_read | fs::perms::owner_write),
            "Output component is not 0600");
    run({"export", backup, seed}, false);
    run({"unpack", seed, output}, false);
    check(read(seed) == original, "Existing seed was replaced");
    const auto link = root.filePath("destination-link");
    fs::create_symlink(seed.toStdString(), link.toStdString());
    run({"export", backup, link}, false);
    const auto dangling = root.filePath("dangling-link");
    fs::create_symlink(root.filePath("missing").toStdString(), dangling.toStdString());
    run({"export", backup, dangling}, false);
    run({"unpack", seed, dangling}, false);

    auto reject_text = [&](const QByteArray& text) {
      const auto edited = root.filePath("edited.yaml");
      const auto destination = root.filePath("must-not-exist");
      write(edited, text);
      run({"unpack", edited, destination}, false);
      check(!QFileInfo::exists(destination), "Rejected input left published output");
    };
    auto reject = [&](const std::function<void(YAML::Node&)>& change) {
      auto edited = YAML::Clone(manifest);
      change(edited);
      reject_text(QByteArray::fromStdString(YAML::Dump(edited)));
    };
    reject([](auto& node) { node["keys"]["otp"]["nand_aes_key"] = "not hex"; });
    reject([](auto& node) { node["keys"]["otp"]["nand_aes_key"] = std::string(32, '0'); });
    reject([](auto& node) { node["keys"]["otp"]["ng_private_key"] = std::string(60, '0'); });
    reject([](auto& node) { node["keys"]["extra"] = YAML::Node(YAML::NodeType::Sequence); });
    reject([](auto& node) {
      YAML::Node entry;
      entry["offset"] = 0x158;
      entry["hex"] = "00";
      node["keys"]["extra"].push_back(entry);
    });
    reject([](auto& node) { node["bad_blocks"].push_back(42); });
    reject([](auto& node) { node["bad_blocks"].push_back(4096); });
    reject([](auto& node) { node["settings"]["fields"]["AREA"] = "USA"; });
    reject([](auto& node) { node["keys"]["_sha256"] = std::string(64, '0'); });
    reject([](auto& node) { node["boot_blocks"]["_sha256"] = std::string(64, '0'); });
    reject([](auto& node) { node["settings"]["_sha256"] = std::string(64, '0'); });
    reject([](auto& node) { node["boot_blocks"]["sha256"] = std::string(64, '0'); });
    reject([](auto& node) {
      const auto text = node["boot_blocks"]["xz"].template as<std::string>();
      node["boot_blocks"].remove("xz");
      node["boot_blocks"]["xz"] = text;
    });
    reject([](auto& node) {
      const auto packed = node["boot_blocks"]["xz"].template as<YAML::Binary>();
      QByteArray damaged(reinterpret_cast<const char*>(packed.data()), packed.size());
      damaged[damaged.size() / 2] = damaged[damaged.size() / 2] ^ 1;
      node["boot_blocks"]["xz"] = YAML::Binary(reinterpret_cast<const unsigned char*>(damaged.data()), damaged.size());
      node["boot_blocks"]["xz"].SetTag("tag:yaml.org,2002:binary");
    });
    reject([](auto& node) { node["sops"]["mac"] = "not decrypted"; });
    reject_text(original + "\nkeys: duplicate\n");
    reject_text(original + "\n---\nsecond: document\n");
    reject_text(original + "\nloop: &loop {again: *loop}\n");
    reject_text(QByteArray(2 * 1024 * 1024 + 1, ' '));

    // A noncanonical tail cannot be inferred from the readable settings map.
    auto odd_settings = settings;
    odd_settings[255] = '\x7f';
    {
      NandBin nand;
      check(nand.SetPath(backup) && nand.InitNand() && nand.SetData(setting_path, odd_settings) && nand.WriteMetaData(),
            "Cannot write noncanonical fixture settings");
    }
    const auto odd_seed = root.filePath("odd.seed.yaml");
    const auto odd_output = root.filePath("odd-output");
    run({"export", backup, odd_seed});
    const auto odd_manifest = YAML::Load(read(odd_seed).toStdString());
    check(odd_manifest["settings"]["original"].IsScalar(), "Noncanonical settings lost original bytes");
    run({"unpack", odd_seed, odd_output});
    check(read(odd_output + "/setting.txt") == odd_settings, "Settings fallback changed bytes");
    // SOPS drops the binary tag when the decoded bytes are valid UTF-8.
    auto decoded = YAML::Clone(manifest);
    const QByteArray zero_settings(256, '\0');
    decoded["settings"]["fields"] = YAML::Node(YAML::NodeType::Map);
    decoded["settings"]["original"] = std::string(256, '\0');
    decoded["settings"]["_sha256"] =
        QCryptographicHash::hash(zero_settings, QCryptographicHash::Sha256).toHex().toStdString();
    const auto decoded_seed = root.filePath("decoded.seed.yaml");
    const auto decoded_output = root.filePath("decoded-output");
    write(decoded_seed, QByteArray::fromStdString(YAML::Dump(decoded)));
    run({"unpack", decoded_seed, decoded_output});
    check(read(decoded_output + "/setting.txt") == zero_settings, "SOPS-decoded UTF-8 binary bytes changed");
    {
      NandBin nand;
      check(nand.SetPath(backup) && nand.InitNand() && nand.Delete(setting_path) && nand.WriteMetaData(),
            "Cannot remove fixture settings");
    }
    const auto absent_seed = root.filePath("absent.seed.yaml");
    const auto absent_output = root.filePath("absent-output");
    run({"export", backup, absent_seed});
    run({"unpack", absent_seed, absent_output});
    check(!QFileInfo::exists(absent_output + "/setting.txt"), "Missing settings were fabricated");

    {
      QFile file(backup);
      check(file.open(QIODevice::ReadWrite) && file.resize(0x21000000), "Cannot create sidecar-key fixture");
    }
    write(root.filePath("keys.bin"), keys);
    const auto sidecar_seed = root.filePath("sidecar.seed.yaml");
    const auto sidecar_output = root.filePath("sidecar-output");
    run({"export", backup, sidecar_seed});
    run({"unpack", sidecar_seed, sidecar_output});
    check(read(sidecar_output + "/keys.bin") == keys && read(sidecar_output + "/boot-blocks.bin") == boots,
          "Sidecar-key round-trip changed bytes");
    auto wrong_keys = keys;
    wrong_keys[0x144] = wrong_keys[0x144] ^ 1;
    write(root.filePath("keys.bin"), wrong_keys);
    const auto wrong_output = root.filePath("wrong.seed.yaml");
    run({"export", backup, wrong_output}, false);
    check(!QFileInfo::exists(wrong_output), "Wrong HMAC key produced a published seed");
    // Allow external SOPS checks to use the same synthetic seed, never a console backup.
    if (argc == 3) check(QFile::copy(seed, app.arguments()[2]), "Cannot copy synthetic seed to a new file");
    std::cout << "Lossless YAML/key/boot/settings round-trips and unsafe-input regressions passed.\n";
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
