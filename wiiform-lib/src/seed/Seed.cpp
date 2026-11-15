// SPDX-License-Identifier: GPL-2.0-only
#include <Seed.hpp>
#include <Layout.hpp>
#include <NandBuffer.hpp>
#include <WorkingImage.hpp>
#include <BootFlags.hpp>
#include <nandbin.h>

#include <QCryptographicHash>
#include <QTreeWidgetItem>
#include <QtEndian>

#include <array>
#include <cstring>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <string>

namespace {
  constexpr int boot_size = 0x108000;
  constexpr int keys_size = 0x400;
  struct SettingField {
    const char* name;
    std::string wiiform::ConsoleSettings::*member;
  };
  constexpr SettingField setting_fields[] = {
      {"AREA", &wiiform::ConsoleSettings::area}, {"MODEL", &wiiform::ConsoleSettings::model},
      {"DVD", &wiiform::ConsoleSettings::dvd}, {"MPCH", &wiiform::ConsoleSettings::mpch},
      {"CODE", &wiiform::ConsoleSettings::code}, {"SERNO", &wiiform::ConsoleSettings::serial},
      {"VIDEO", &wiiform::ConsoleSettings::video}, {"GAME", &wiiform::ConsoleSettings::game}};

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto digest(const QByteArray& data) -> wiiform::Sha256 {
    const auto hash = QCryptographicHash::hash(data, QCryptographicHash::Sha256);
    wiiform::Sha256 result{};
    std::memcpy(result.data(), hash.constData(), result.size());
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

  auto bootmii_15_marker(const QByteArray& boots, int first_block) -> bool {
    constexpr int page_size = 0x840;
    constexpr int data_size = 0x800;
    constexpr int pages_per_block = 64;
    constexpr int block_size = data_size * pages_per_block;
    if (boots.size() != boot_size || first_block < 0 || first_block >= 8) return false;

    QByteArray first(block_size, '\0');
    for (int page = 0; page < pages_per_block; ++page) {
      const auto source = (first_block * pages_per_block + page) * page_size;
      first.replace(page * data_size, data_size, boots.constData() + source, data_size);
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

  auto footer_header(const QByteArray& keys) -> std::string {
    int size = 0;
    while (size < 0x100 && keys[size] != '\0' && static_cast<unsigned char>(keys[size]) >= 0x20 &&
           static_cast<unsigned char>(keys[size]) <= 0x7e)
      ++size;
    return std::string(keys.constData(), static_cast<std::size_t>(size));
  }

} // namespace

namespace seed {
  auto encode_settings(const wiiform::ConsoleSettings& settings) -> QByteArray {
    QByteArray plain;
    for (const auto& field : setting_fields) {
      const auto& value = settings.*(field.member);
      require(!value.empty(), "Console setting values must not be empty");
      require(value.size() <= 256, "Console setting value exceeds setting.txt capacity");
      for (const auto ch : value)
        require(static_cast<unsigned char>(ch) >= 0x20 && static_cast<unsigned char>(ch) <= 0x7e,
                "Console settings must contain printable ASCII only");
      plain += QByteArray(field.name) + '=' + QByteArray(value.data(), int(value.size())) + "\r\n";
    }
    require(plain.size() <= 256, "Canonical console settings exceed setting.txt capacity");
    plain.append(QByteArray(256 - plain.size(), '\0'));
    return crypt_settings(plain);
  }

  auto decode_settings(const QByteArray& raw) -> wiiform::ConsoleSettings {
    require(raw.size() == 256, "Expected a 256-byte setting.txt");
    auto plain = crypt_settings(raw);
    const auto end = plain.indexOf('\0');
    require(end >= 0, "setting.txt has no NUL terminator");
    plain.truncate(end);

    wiiform::ConsoleSettings settings;
    int cursor = 0;
    for (const auto& field : setting_fields) {
      const auto prefix = QByteArray(field.name) + '=';
      require(plain.mid(cursor, prefix.size()) == prefix, "setting.txt fields are missing or out of order");
      cursor += prefix.size();
      const auto line_end = plain.indexOf("\r\n", cursor);
      require(line_end > cursor, "Malformed or empty setting.txt value");
      settings.*(field.member) = plain.mid(cursor, line_end - cursor).toStdString();
      cursor = line_end + 2;
    }
    require(cursor == plain.size() && encode_settings(settings) == raw,
            "setting.txt is not in canonical Wii format");
    return settings;
  }

  auto extract(std::span<const std::uint8_t> backup, std::span<const std::uint8_t> sidecar)
      -> wiiform::SeedParts {
    require((backup.size() == 0x21000000 && sidecar.size() == keys_size) ||
                (backup.size() == 0x21000400 && sidecar.empty()),
            "Expected an ECC-bearing NAND with an embedded footer or a matching 1024-byte sidecar");
    inspect::validate_layout(backup, false);
    nand_buffer::Device device(backup);
    NandBin nand;
    const auto keys = QByteArray(reinterpret_cast<const char*>(sidecar.data()), int(sidecar.size()));
    require(nand.SetDevice(&device, keys), "Cannot open NAND backup buffer");
    require(nand.InitNand(), "Cannot read NAND filesystem");
    bool authenticated = false;
    for (int cluster = 0x7f00; cluster < 0x8000 && !authenticated; cluster += 0x10)
      authenticated = nand.CheckHmacMeta(cluster);
    require(authenticated, "Keys do not authenticate any filesystem superblock");

    QByteArray boots(boot_size, '\0');
    require(device.seek(0) && device.read(boots.data(), boot_size) == boot_size, "Cannot read boot blocks");
    const auto all_keys = nand.Keys();
    require(all_keys.size() == keys_size, "Expected exactly 1024 key footer bytes");
    const auto fats = nand.GetFats();
    require(fats.size() == 0x8000, "Unexpected allocation-table size");

    wiiform::SeedParts result;
    result.keys.assign(reinterpret_cast<const std::uint8_t*>(all_keys.constData()),
                       reinterpret_cast<const std::uint8_t*>(all_keys.constData()) + all_keys.size());
    result.boot_blocks.assign(reinterpret_cast<const std::uint8_t*>(boots.constData()),
                              reinterpret_cast<const std::uint8_t*>(boots.constData()) + boots.size());
    result.footer_header = footer_header(all_keys);
    for (int block = 8; block < 4096; ++block) {
      int bad = 0;
      for (int i = 0; i < 8; ++i) bad += fats[block * 8 + i] == 0xfffd;
      require(!bad || bad == 8, "Inconsistent bad-block markers");
      if (bad) result.bad_blocks.push_back(static_cast<std::uint16_t>(block));
    }

    std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    require(tree && tree->childCount() == 1, "Unexpected NAND root");
    const auto* settings_node = working_image::find_path(tree.get(), "/title/00000001/00000002/data/setting.txt");
    const auto setting = working_image::read_authenticated_file(nand, settings_node, 4096);
    result.settings = decode_settings(setting);
    result.settings_sha256 = digest(setting);

    bool detected_bootmii = false;
    bool all_copies_stock = true;
    bool stock_primary = false;
    for (const auto& info : nand.Boot2Infos()) {
      std::string state = "unknown";
      if (info.state < 0 || boot::has_bad_content_hash(info.state)) state = "invalid";
      else if ((info.state & (BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK)) == (BOOT_2_TMD_SIG_OK | BOOT_2_TIK_SIG_OK))
        state = "stock";
      else if (info.state & (BOOT_2_TMD_FAKESIGNED | BOOT_2_TIK_FAKESIGNED)) state = "fakesigned";
      const bool stock = state == "stock";
      all_copies_stock &= stock;
      if (stock && (info.state & BOOT_2_USED_TO_BOOT) && !(info.state & BOOT_2_MARKED_BAD)) stock_primary = true;
      if (state == "fakesigned" && (info.state & BOOT_2_TMD_FAKESIGNED) &&
          (info.version == BOOTMII_11 || info.version == BOOTMII_13 ||
           (info.version == BOOTMII_UNK && bootmii_15_marker(boots, info.firstBlock))) &&
          !(info.state & BOOT_2_MARKED_BAD))
        detected_bootmii = true;
    }
    result.bootmii = detected_bootmii ? wiiform::BootMiiState::detected
        : all_copies_stock && stock_primary ? wiiform::BootMiiState::stock
        : wiiform::BootMiiState::unknown;
    return result;
  }
} // namespace seed
