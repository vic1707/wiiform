// SPDX-License-Identifier: GPL-2.0-only
#include <Seed.hpp>
#include <Support.hpp>
#include <Layout.hpp>
#include <SysConf.hpp>
#include <WifiConfig.hpp>
#include <NandBuffer.hpp>
#include <WorkingImage.hpp>
#include <BootFlags.hpp>
#include <Titles.hpp>
#include <TitleValidation.hpp>
#include <tiktmd.h>
#include <nandbin.h>
#include <tools.h>

#include <QCryptographicHash>

#include <cstring>
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

  // The BootMii marker a boot2 copy carries, as the TMD does; empty when unknown.
  auto boot2_marker(auto version) -> std::string {
    if (version == BOOTMII_15) return "BM1.5";
    if (version == BOOTMII_13) return "BM1.3";
    if (version == BOOTMII_11) return "BM1.1";
    return {};
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
    return SettingTxtCrypt(plain);
  }

  auto decode_settings(const QByteArray& raw) -> wiiform::ConsoleSettings {
    require(raw.size() == 256, "Expected a 256-byte setting.txt");
    auto plain = SettingTxtCrypt(raw);
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

  auto bytes_of(const QByteArray& bytes) -> std::vector<std::uint8_t> {
    return {reinterpret_cast<const std::uint8_t*>(bytes.constData()),
            reinterpret_cast<const std::uint8_t*>(bytes.constData()) + bytes.size()};
  }

  // Keeps BootMii as IOS when the backup has it: IOS254 with BootMii's marker.
  void read_bootmii_ios(NandBin& nand, wiiform::SeedParts& result) {
    using working_image::find_path;
    using working_image::read_authenticated_file;
    const QString folder = "/title/00000001/000000fe/content/";
    const auto tmd_entry = find_path(nand, folder + "title.tmd");
    if (!tmd_entry) return;
    const auto tmd = read_authenticated_file(nand, tmd_entry, 64 * 1024);
    if (!wiiform::has_bootmii_marker(tmd)) return;
    Tmd parsed(tmd);
    require(parsed.Count() == 1 && parsed.Type(0) == 1, "BootMii as IOS has an unexpected content layout");
    NusJob job;
    job.tid = wiiform::bootmii_ios_title_id;
    job.version = parsed.Version();
    job.decrypt = true;
    job.data << tmd << read_authenticated_file(nand, "/ticket/00000001/000000fe.tik", 64 * 1024)
             << read_authenticated_file(nand, folder + parsed.Cid(0) + ".app", parsed.Size(0));
    const auto validated = validate_title(std::move(job), "The backup's BootMii as IOS is inconsistent");
    const auto& parts = validated.get().data;
    result.bootmii_ios_tmd = bytes_of(parts[0]);
    result.bootmii_ios_ticket = bytes_of(parts[1]);
    result.bootmii_ios_content = bytes_of(parts[2]);
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
    for (int block = 8; block < 4080; ++block)
      if (fats[block * 8] == 0xfffd)
        result.bad_blocks.push_back(static_cast<std::uint16_t>(block));

    const auto setting =
        working_image::read_authenticated_file(nand, "/title/00000001/00000002/data/setting.txt", 4096);
    result.settings = decode_settings(setting);
    result.settings_sha256 = sha256(setting);

    // Wi-Fi and Wiimote state, when the console has any. The Wiimote registrations are
    // one entry of SYSCONF, which a console without setup does not have.
    if (const auto profile = working_image::find_path(nand, wifi_config::path)) {
      const auto config = working_image::read_authenticated_file(nand, profile, wifi_config::size);
      require(config.size() == wifi_config::size, "Wi-Fi profile file has the wrong size");
      result.wifi_config = bytes_of(config);
    }
    if (const auto sysconf = working_image::find_path(nand, wii_sysconf::path)) {
      const auto image = working_image::read_authenticated_file(nand, sysconf, wii_sysconf::size);
      if (const auto registrations = wii_sysconf::find_entry(image, "BT.DINF")) {
        require(registrations->size() == wii_sysconf::registrations_size, "Wiimote registrations have the wrong size");
        result.wiimote_registrations = bytes_of(*registrations);
      }
    }

    bool detected_bootmii = false;
    bool all_copies_stock = true;
    bool stock_primary = false;
    // Empty when detected copies disagree about their version.
    std::string boot2_version;
    bool boot2_versions_agree = true;
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
          (info.version == BOOTMII_11 || info.version == BOOTMII_13 || info.version == BOOTMII_15) &&
          !(info.state & BOOT_2_MARKED_BAD)) {
        detected_bootmii = true;
        const auto marker = boot2_marker(info.version);
        if (!boot2_version.empty() && boot2_version != marker) boot2_versions_agree = false;
        boot2_version = marker;
      }
    }
    result.bootmii = detected_bootmii ? wiiform::BootMiiState::detected
        : all_copies_stock && stock_primary ? wiiform::BootMiiState::stock
        : wiiform::BootMiiState::unknown;
    result.bootmii_boot2_version = detected_bootmii && boot2_versions_agree ? boot2_version : std::string{};
    read_bootmii_ios(nand, result);
    return result;
  }
} // namespace seed
