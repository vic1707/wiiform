// SPDX-License-Identifier: GPL-2.0-only
#include <Seed.hpp>
#include <BootFlags.hpp>
#include <nandbin.h>

#include <QCryptographicHash>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QtEndian>
#include <lzma.h>
#include <yaml-cpp/yaml.h>

#include <array>
#include <memory>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>

namespace {
  constexpr int boot_size = 0x108000;
  constexpr int keys_size = 0x400;
  constexpr int manifest_limit = 2 * 1024 * 1024;
  constexpr std::array setting_fields = {"AREA", "MODEL", "DVD", "MPCH", "CODE", "SERNO", "VIDEO", "GAME"};

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto sha256(const QByteArray& data) -> std::string {
    return QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex().toStdString();
  }

  auto read_file(const QString& path, qint64 limit) -> QByteArray {
    QFile file(path);
    require(file.open(QIODevice::ReadOnly), "Cannot open input file");
    const auto size = file.size();
    require(size >= 0 && size <= limit, "Input exceeds size limit");
    const auto data = file.read(limit + 1);
    require(file.error() == QFileDevice::NoError && data.size() == size, "Incomplete input read");
    return data;
  }

  auto latin1(const QByteArray& data) -> std::string {
    // Qt 5's QByteArray overload truncates embedded NULs.
    return QString::fromLatin1(data.constData(), data.size()).toStdString();
  }

  auto from_latin1(const std::string& text) -> QByteArray {
    const auto data = QString::fromStdString(text).toLatin1();
    require(latin1(data) == text, "Value is not losslessly representable as Latin-1");
    return data;
  }

  // The pinned WiiQt parser reports BOOTMII_UNK for BM1.5. Look for the marker
  // within the TMD of the boot2 copy that passed the content hash check.
  auto bootmii_15_marker(const QByteArray& raw_boots, int first_block) -> bool {
    constexpr int page_size = 0x840;
    constexpr int data_size = 0x800;
    constexpr int pages_per_block = 64;
    constexpr int logical_block_size = data_size * pages_per_block;
    if (raw_boots.size() != boot_size || first_block < 0 || first_block >= 8) return false;

    QByteArray first(logical_block_size, '\0');
    for (int page = 0; page < pages_per_block; ++page) {
      const auto source = (first_block * pages_per_block + page) * page_size;
      first.replace(page * data_size, data_size, raw_boots.constData() + source, data_size);
    }
    const auto* header = first.constData();
    const auto header_size = qFromBigEndian<quint32>(header);
    const quint64 cert_size = qFromBigEndian<quint32>(header + 8);
    const quint64 ticket_size = qFromBigEndian<quint32>(header + 12);
    const quint64 tmd_size = qFromBigEndian<quint32>(header + 16);
    const quint64 tmd_offset = quint64(header_size) + cert_size + ticket_size;
    if (header_size != 0x20 || tmd_size == 0 || tmd_offset > quint64(first.size()) ||
        tmd_size > quint64(first.size()) - tmd_offset)
      return false;
    return first.mid(static_cast<int>(tmd_offset), static_cast<int>(tmd_size)).contains("BM1.5");
  }

  auto yaml_string(const std::string& value) -> YAML::Node {
    YAML::Node node(value);
    // Prevent SOPS from interpreting leading-zero hex and serials as numbers.
    node.SetTag("tag:yaml.org,2002:str");
    return node;
  }

  auto hex(const YAML::Node& node, int size) -> QByteArray {
    const auto text = node.as<std::string>();
    require(text.size() == static_cast<size_t>(size) * 2, "Invalid hex field length");
    const auto data = QByteArray::fromHex(QByteArray::fromStdString(text));
    require(data.size() == size && data.toHex().toStdString() == QString::fromStdString(text).toLower().toStdString(),
            "Invalid hex field");
    return data;
  }

  auto binary(const QByteArray& data) -> YAML::Node {
    YAML::Node node(YAML::Binary(reinterpret_cast<const unsigned char*>(data.constData()), data.size()));
    node.SetTag("tag:yaml.org,2002:binary");
    return node;
  }

  void emit_yaml(YAML::Emitter& emitter, const YAML::Node& node) {
    if (node.IsMap()) {
      emitter << YAML::BeginMap;
      for (const auto& entry : node) {
        emitter << YAML::Key << entry.first << YAML::Value;
        emit_yaml(emitter, entry.second);
      }
      emitter << YAML::EndMap;
    } else if (node.IsSequence()) {
      emitter << YAML::BeginSeq;
      for (const auto& entry : node) emit_yaml(emitter, entry);
      emitter << YAML::EndSeq;
    } else if (node.Tag() == "tag:yaml.org,2002:str") emitter << YAML::DoubleQuoted << node.as<std::string>();
    else if (node.Tag() == "tag:yaml.org,2002:binary") emitter << node.as<YAML::Binary>();
    else emitter << node;
  }

  auto binary(const YAML::Node& node, size_t limit) -> QByteArray {
    require(node.IsScalar(), "Expected a binary scalar");
    QByteArray result;
    if (node.Tag() == "tag:yaml.org,2002:binary") {
      const auto data = node.as<YAML::Binary>();
      result = QByteArray(reinterpret_cast<const char*>(data.data()), static_cast<int>(data.size()));
    } else {
      // SOPS emits valid-UTF-8 binary payloads as ordinary strings after decryption.
      result = QByteArray::fromStdString(node.as<std::string>());
    }
    require(static_cast<size_t>(result.size()) <= limit, "Binary value exceeds size limit");
    return result;
  }

  void validate_yaml(const YAML::Node& node, int depth, int& remaining) {
    require(depth <= 16 && --remaining >= 0, "YAML nesting or node limit exceeded");
    if (node.IsMap()) {
      std::set<std::string> keys;
      for (const auto& entry : node) {
        require(entry.first.IsScalar() && keys.insert(entry.first.as<std::string>()).second,
                "Duplicate or non-scalar YAML key");
        validate_yaml(entry.second, depth + 1, remaining);
      }
    } else if (node.IsSequence()) {
      for (const auto& entry : node) validate_yaml(entry, depth + 1, remaining);
    }
  }

  struct Field {
    const char* section;
    const char* name;
    int offset;
    int size;
  };
  constexpr Field fields[] = {{"otp", "boot1_sha1", 0x100, 20},
                              {"otp", "common_key", 0x114, 16},
                              {"otp", "console_id", 0x124, 4},
                              {"otp", "ng_private_key", 0x128, 30},
                              {"otp", "nand_hmac_key", 0x144, 20},
                              {"otp", "nand_aes_key", 0x158, 16},
                              {"otp", "rng_key", 0x168, 16},
                              {"otp", "unknown_78", 0x178, 8},
                              {"seeprom", "ms_id", 0x200, 4},
                              {"seeprom", "ca_id", 0x204, 4},
                              {"seeprom", "ng_key_id", 0x208, 4},
                              {"seeprom", "ng_signature", 0x20c, 60},
                              {"seeprom", "boot2_counters", 0x248, 20},
                              {"seeprom", "nand_counters", 0x25c, 18},
                              {"seeprom", "korean_key", 0x274, 16},
                              {"seeprom", "prng_seed", 0x2f8, 4}};

  auto describe_keys(const QByteArray& keys) -> YAML::Node {
    require(keys.size() == keys_size, "Expected exactly 1024 key bytes");
    QByteArray known(keys_size, '\0');
    int header_size = 0;
    while (header_size < 0x100 && keys[header_size] != '\0' && static_cast<unsigned char>(keys[header_size]) < 0x7f)
      ++header_size;
    const auto header = keys.left(header_size);
    YAML::Node result;
    result["header"] = yaml_string(latin1(header));
    known.replace(0, header_size, QByteArray(header_size, '\1'));
    for (const auto& field : fields) {
      result[field.section][field.name] = yaml_string(keys.mid(field.offset, field.size).toHex().toStdString());
      known.replace(field.offset, field.size, QByteArray(field.size, '\1'));
    }
    result["extra"] = YAML::Node(YAML::NodeType::Sequence);
    for (int i = 0; i < keys_size;) {
      if (known[i] || keys[i] == '\0') {
        ++i;
        continue;
      }
      const auto start = i;
      while (i < keys_size && !known[i] && keys[i] != '\0') ++i;
      YAML::Node entry;
      entry["offset"] = start;
      entry["hex"] = yaml_string(keys.mid(start, i - start).toHex().toStdString());
      result["extra"].push_back(entry);
    }
    return result;
  }

  auto restore_keys(const YAML::Node& node) -> QByteArray {
    const auto header = from_latin1(node["header"].as<std::string>());
    require(header.size() <= 0x100, "Key header exceeds size limit");
    QByteArray keys(keys_size, '\0');
    keys.replace(0, header.size(), header);
    QByteArray known(keys_size, '\0');
    known.replace(0, header.size(), QByteArray(header.size(), '\1'));
    for (const auto& field : fields) {
      const auto data = hex(node[field.section][field.name], field.size);
      for (int i = 0; i < field.size; ++i) {
        const auto position = field.offset + i;
        require(!known[position] || keys[position] == data[i], "Overlapping key fields disagree");
        keys[position] = data[i];
        known[position] = '\1';
      }
    }
    require(node["extra"].IsSequence(), "Missing unknown-byte records");
    for (const auto& entry : node["extra"]) {
      const auto offset = entry["offset"].as<int>();
      const auto text = entry["hex"].as<std::string>();
      require(offset >= 0 && !text.empty() && text.size() % 2 == 0 && text.size() <= keys_size * 2,
              "Invalid unknown-byte range");
      const auto data = hex(entry["hex"], static_cast<int>(text.size() / 2));
      require(offset <= keys_size - data.size(), "Unknown-byte range exceeds keys.bin");
      for (int i = 0; i < data.size(); ++i) {
        require(!known[offset + i], "Unknown-byte record overlaps another field");
        keys[offset + i] = data[i];
        known[offset + i] = '\1';
      }
    }
    return keys;
  }

  auto compress(const QByteArray& data) -> QByteArray {
    QByteArray result(static_cast<int>(lzma_stream_buffer_bound(data.size())), '\0');
    size_t size = 0;
    require(lzma_easy_buffer_encode(6, LZMA_CHECK_CRC64, nullptr, reinterpret_cast<const uint8_t*>(data.constData()),
                                    data.size(), reinterpret_cast<uint8_t*>(result.data()), &size,
                                    result.size()) == LZMA_OK,
            "Boot compression failed");
    result.resize(static_cast<int>(size));
    return result;
  }

  auto decompress(const QByteArray& data) -> QByteArray {
    QByteArray result(boot_size, '\0');
    size_t input = 0, output = 0;
    uint64_t memory_limit = 128 * 1024 * 1024;
    const auto status =
        lzma_stream_buffer_decode(&memory_limit, 0, nullptr, reinterpret_cast<const uint8_t*>(data.constData()), &input,
                                  data.size(), reinterpret_cast<uint8_t*>(result.data()), &output, result.size());
    require(status == LZMA_OK && input == static_cast<size_t>(data.size()) && output == boot_size,
            "Invalid compressed boot blocks (size, checksum or memory limit)");
    return result;
  }

  auto crypt_settings(QByteArray data) -> QByteArray {
    auto end = data.size();
    while (end && data[end - 1] == '\0') --end;
    quint32 key = 0x73b5dbfa;
    for (int i = 0; i < end; ++i) {
      data[i] = data[i] ^ static_cast<char>(key & 0xff);
      key = (key << 1) | (key >> 31);
    }
    return data;
  }

  auto describe_settings(const QByteArray& raw) -> YAML::Node {
    YAML::Node result(YAML::NodeType::Map);
    if (raw.size() != 256) return result;
    auto plain = crypt_settings(raw);
    const auto zero = plain.indexOf('\0');
    if (zero >= 0) plain.truncate(zero);
    for (const auto& line : QString::fromLatin1(plain).split('\n', Qt::SkipEmptyParts)) {
      const auto text = line.trimmed();
      const auto equals = text.indexOf('=');
      if (equals > 0) result[text.left(equals).toStdString()] = yaml_string(text.mid(equals + 1).toStdString());
    }
    return result;
  }

  auto restore_settings(const YAML::Node& settings) -> QByteArray {
    if (!settings.IsMap() || settings.size() != setting_fields.size()) return {};
    QByteArray plain;
    for (const auto* name : setting_fields) {
      if (!settings[name].IsScalar()) return {};
      const auto value = from_latin1(settings[name].as<std::string>());
      for (const auto byte : value)
        if (static_cast<unsigned char>(byte) < 0x20 || static_cast<unsigned char>(byte) >= 0x7f) return {};
      plain += QByteArray(name) + '=' + value + "\r\n";
    }
    if (plain.size() > 256) return {};
    plain.append(QByteArray(256 - plain.size(), '\0'));
    return crypt_settings(plain);
  }

  auto destination_info(const QString& path) -> QFileInfo {
    const QFileInfo destination(QDir::cleanPath(QFileInfo(path).absoluteFilePath()));
    require(!destination.exists() && !destination.isSymLink(), "Destination already exists");
    require(destination.absoluteDir().exists(), "Destination parent must exist");
    return destination;
  }

  void write_file(QFile& file, const QByteArray& data) {
    require(file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner) && file.write(data) == data.size() &&
                file.flush(),
            "Cannot finish private output");
  }
} // namespace

namespace seed {
  auto encode_settings(const YAML::Node& settings) -> QByteArray {
    const auto bytes = restore_settings(settings);
    require(bytes.size() == 256, "Cannot encode canonical console settings");
    return bytes;
  }

  void export_backup(const QString& backup, const QString& output) {
    const auto destination = destination_info(output);
    QFile source(QFileInfo(backup).absoluteFilePath());
    require(source.open(QIODevice::ReadOnly), "Cannot open NAND backup");
    require(source.size() == 0x21000000 || source.size() == 0x21000400, "Expected a BootMii dump with spare data");
    if (source.size() == 0x21000000)
      require(QFileInfo(QFileInfo(source).absoluteDir().filePath("keys.bin")).size() == keys_size,
              "Expected a matching 1024-byte keys.bin beside the dump");
    NandBin nand;
    require(nand.SetPath(source.fileName()) && nand.InitNand(), "Cannot read NAND filesystem");
    const auto boots = source.read(boot_size);
    require(boots.size() == boot_size, "Cannot read boot blocks");
    const auto keys = nand.Keys();
    const auto key_fields = describe_keys(keys);
    require(restore_keys(key_fields) == keys, "Key reconstruction self-check failed");
    bool authenticated = false;
    for (int cluster = 0x7f00; cluster < 0x8000 && !authenticated; cluster += 0x10)
      authenticated = nand.CheckHmacMeta(cluster);
    require(authenticated, "Keys do not authenticate any filesystem superblock");

    YAML::Node manifest;
    QCryptographicHash source_hash(QCryptographicHash::Sha256);
    require(source.seek(0) && source_hash.addData(&source), "Cannot hash NAND backup");
    manifest["source"]["sha256"] = yaml_string(source_hash.result().toHex().toStdString());
    manifest["keys"] = key_fields;
    manifest["keys"]["_sha256"] = yaml_string(sha256(keys));
    manifest["bad_blocks"] = YAML::Node(YAML::NodeType::Sequence);
    const auto fats = nand.GetFats();
    require(fats.size() == 0x8000, "Unexpected allocation-table size");
    for (int block = 8; block < 4096; ++block) {
      int bad = 0;
      for (int i = 0; i < 8; ++i) bad += fats[block * 8 + i] == 0xfffd;
      require(!bad || bad == 8, "Inconsistent bad-block markers");
      if (bad) manifest["bad_blocks"].push_back(block);
    }
    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    require(tree->childCount() == 1, "Unexpected NAND root");
    manifest["boot1_version"] = static_cast<int>(nand.Boot1Version());
    manifest["boot2"] = YAML::Node(YAML::NodeType::Sequence);
    bool detected_bootmii = false;
    bool all_copies_stock = true;
    bool stock_primary = false;
    for (const auto& info : nand.Boot2Infos()) {
      std::string state = "unknown";
      if (info.state < 0 || boot::has_bad_content_hash(info.state)) state = "invalid";
      else if ((info.state & (BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK)) == (BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK))
        state = "stock";
      else if (info.state & (BOOT_2_TMD_FAKESIGNED | BOOT_2_TIK_FAKESIGNED)) state = "fakesigned";
      YAML::Node copy;
      copy["blocks"].push_back(static_cast<int>(info.firstBlock));
      copy["blocks"].push_back(static_cast<int>(info.secondBlock));
      copy["version"] = info.state >= 0 ? info.version : 0;
      copy["state"] = state;
      if (info.state >= 0 && (info.state & BOOT_2_USED_TO_BOOT)) copy["used_to_boot"] = true;
      if (info.state >= 0 && (info.state & BOOT_2_MARKED_BAD)) copy["marked_bad"] = true;
      const bool stock = state == "stock";
      all_copies_stock &= stock;
      if (stock && (info.state & BOOT_2_USED_TO_BOOT) && !(info.state & BOOT_2_MARKED_BAD))
        stock_primary = true;
      if (state == "fakesigned" && (info.state & BOOT_2_TMD_FAKESIGNED) &&
          (info.version == BOOTMII_11 || info.version == BOOTMII_13 ||
           (info.version == BOOTMII_UNK && bootmii_15_marker(boots, info.firstBlock)))) {
        const auto marker = info.version == BOOTMII_11 ? "BM1.1" :
                            info.version == BOOTMII_13 ? "BM1.3" : "BM1.5";
        copy["bootmii_marker"] = yaml_string(marker);
        if (!(info.state & BOOT_2_MARKED_BAD)) detected_bootmii = true;
      }
      manifest["boot2"].push_back(copy);
    }
    manifest["bootmii_boot2"] = detected_bootmii ? "detected" :
                                all_copies_stock && stock_primary ? "stock" : "unknown";
    const auto packed = compress(boots);
    require(decompress(packed) == boots, "Compression self-check failed");

    auto* setting_entry = tree->child(0);
    for (const auto& name : QString("title/00000001/00000002/data/setting.txt").split('/')) {
      QTreeWidgetItem* next = nullptr;
      for (int i = 0; setting_entry && i < setting_entry->childCount(); ++i)
        if (setting_entry->child(i)->text(0) == name) next = setting_entry->child(i);
      setting_entry = next;
    }
    require(!setting_entry || setting_entry->text(2).toULongLong(nullptr, 16) <= 4096, "Unexpectedly large settings file");
    const auto setting = setting_entry ? nand.GetFile(setting_entry->text(1).toUShort()) : QByteArray();
    require(setting.size() <= 4096, "Unexpectedly large settings file");
    if (!setting.isEmpty()) {
      manifest["settings"]["_sha256"] = yaml_string(sha256(setting));
      manifest["settings"]["fields"] = describe_settings(setting);
      if (restore_settings(manifest["settings"]["fields"]) != setting)
        manifest["settings"]["original"] = binary(setting);
    }
    // Keep the large opaque payload after the readable console and boot metadata.
    manifest["boot_blocks"]["_sha256"] = yaml_string(sha256(boots));
    manifest["boot_blocks"]["xz"] = binary(packed);
    YAML::Emitter emitter;
    emitter.SetOutputCharset(YAML::EscapeNonAscii);
    emit_yaml(emitter, manifest);
    require(emitter.good(), "Cannot encode seed YAML");
    const auto text = std::string(emitter.c_str()) + '\n';
    require(text.size() <= manifest_limit, "Exported seed exceeds size limit");
    QTemporaryFile staging(destination.absoluteDir().filePath(".wii-seed-XXXXXX"));
    require(staging.open(), "Cannot create private staging file");
    write_file(staging, QByteArray::fromStdString(text));
    require(staging.rename(destination.absoluteFilePath()), "Cannot publish seed (destination must be new)");
    staging.setAutoRemove(false);
  }

  auto read(const QString& input) -> Components {
    YAML::Node manifest;
    try {
      const auto documents = YAML::LoadAll(read_file(input, manifest_limit).toStdString());
      require(documents.size() == 1 && documents.front().IsMap(), "Expected one YAML mapping");
      manifest = documents.front();
    } catch (const YAML::Exception&) {
      // yaml-cpp parser messages can quote source text, which may include private fields.
      throw std::runtime_error("Malformed or invalid seed YAML");
    }

    try {
      int remaining = 8192;
      validate_yaml(manifest, 0, remaining);
      require(!manifest["sops"], "Decrypt with the SOPS CLI explicitly before unpacking");
      require(manifest["source"].IsMap(), "Missing source record");
      require(!manifest["source_sha256"] && !manifest["keys_sha256"] &&
                  !manifest["settings_sha256"] && !manifest["setting_original"] &&
                  !manifest["boot_blocks"]["bytes"] && !manifest["boot_blocks"]["sha256"],
              "Seed contains obsolete payload fields. Re-export it from the NAND backup");
      hex(manifest["source"]["sha256"], 32);
      const auto keys = restore_keys(manifest["keys"]);
      require(hex(manifest["keys"]["_sha256"], 32).toHex().toStdString() == sha256(keys),
              "Keys do not match their hash");
      const auto boot = manifest["boot_blocks"];
      const auto boots = decompress(binary(boot["xz"], manifest_limit));
      require(hex(boot["_sha256"], 32).toHex().toStdString() == sha256(boots),
              "Boot blocks do not match their hash");
      QMap<QString, QByteArray> files{{"keys.bin", keys}, {"boot-blocks.bin", boots}};
      require(manifest["bad_blocks"].IsSequence(), "Missing bad-block list");
      std::set<int> seen;
      QByteArray bad_blocks;
      for (const auto& entry : manifest["bad_blocks"]) {
        const auto block = entry.as<int>();
        require(block >= 8 && block < 4096 && seen.insert(block).second, "Invalid or duplicate bad block");
        bad_blocks += QByteArray::number(block) + '\n';
      }
      files["bad-blocks.txt"] = bad_blocks;
      if (manifest["settings"]) {
        const auto original = manifest["settings"]["original"];
        const auto raw = original ? binary(original, 4096) : restore_settings(settings_fields(manifest));
        require(!raw.isEmpty() && hex(manifest["settings"]["_sha256"], 32).toHex().toStdString() == sha256(raw),
                "Reconstructed settings do not match their hash");
        files["setting.txt"] = raw;
      }
      return {manifest, std::move(files)};
    } catch (const YAML::Exception&) {
      // Bad conversions and missing nodes may include a source excerpt in yaml-cpp diagnostics.
      throw std::runtime_error("Invalid seed field or value");
    }
  }

  auto settings_fields(const YAML::Node& manifest) -> YAML::Node {
    return manifest["settings"]["fields"];
  }

  auto has_original_settings(const YAML::Node& manifest) -> bool {
    return static_cast<bool>(manifest["settings"]["original"]);
  }

  auto unpack(const QString& input, const QString& output) -> YAML::Node {
    const auto destination = destination_info(output);
    const auto components = read(input);
    QTemporaryDir staging(destination.absoluteDir().filePath(".wii-seed-XXXXXX"));
    require(staging.isValid(), "Cannot create private staging directory");
    for (auto file = components.files.cbegin(); file != components.files.cend(); ++file) {
      QFile component(QDir(staging.path()).filePath(file.key()));
      require(component.open(QIODevice::WriteOnly | QIODevice::NewOnly), "Cannot create output component");
      write_file(component, file.value());
    }
    require(QDir().rename(staging.path(), destination.absoluteFilePath()), "Cannot publish output directory");
    staging.setAutoRemove(false);
    return components.manifest;
  }
} // namespace seed
