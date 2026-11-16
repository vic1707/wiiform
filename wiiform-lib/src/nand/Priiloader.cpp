// SPDX-License-Identifier: GPL-2.0-only
#include <Priiloader.hpp>

#include <Inspection.hpp>
#include <PriiloaderHacks.hpp>
#include <PriiloaderRelease.hpp>
#include <PriiloaderSettings.hpp>
#include <Titles.hpp>
#include <WorkingImage.hpp>
#include <tiktmd.h>

#include <QCryptographicHash>
#include <QtEndian>
#include <stdexcept>

namespace {
  constexpr int menu_app_limit = 32 * 1024 * 1024;
  constexpr int tmd_limit = 2 * 1024 * 1024;
  using priiloader::release_app_size;
  const QString menu = "/title/00000001/00000002";
  const QString content = menu + "/content/";
  const QString tmd_path = content + "title.tmd";
  const QString original_tmd_path = content + "title_or.tmd";
  const QString ticket_path = "/ticket/00000001/00000002.tik";
  const QString settings_path = menu + "/data/loader.ini";

  using working_image::find_path;
  using working_image::read_authenticated_file;

  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  auto menu_version(const inspect::PriiloaderEvidence& evidence) -> int {
    const int version = evidence.menu_version;
    require((version == 512 || version == 513 || version == 514) && evidence.system_menu_ios == 0x100000050ull,
            "Priiloader offline preparation supports Wii System Menu 4.3J, 4.3U and 4.3E with IOS80");
    return version;
  }

  auto patched_tmd(QByteArray original, const QByteArray& app) -> QByteArray {
    // Callers use a bounded, signed original TMD checked by the inspector.
    Tmd tmd(original);
    for (quint16 i = 0; i < tmd.Count(); ++i) {
      if (tmd.Index(i) != tmd.BootIndex()) continue;
      // Upstream's Wii branch keeps the size and signature bytes as stored.
      // Only access rights and the selected boot record's SHA-1 change.
      qToBigEndian<quint32>(3, reinterpret_cast<uchar*>(original.data() + 0x1d8));
      original.replace(0x1e4 + i * 0x24 + 16, 20, QCryptographicHash::hash(app, QCryptographicHash::Sha1));
      return original;
    }
    throw std::runtime_error("System Menu TMD has no selected boot record");
  }

  auto authentic_ticket(NandBin& nand, const QByteArray& tmd) -> QByteArray {
    const auto ticket = read_authenticated_file(nand, find_path(nand, ticket_path), 0x2a4);
    require(ticket.size() == 0x2a4 && check_title_metadata(tmd, ticket) &&
                check_cert_chain(ticket) == ERROR_SUCCESS && Ticket(ticket, false).Tid() == 0x100000002ull,
            "System Menu ticket is not an authentic stock ticket");
    return ticket;
  }

  void check_destinations(NandBin& nand, const wiiform::PriiloaderSettings& settings) {
    using wiiform::Autoboot;
    if (settings.autoboot == Autoboot::file || settings.return_to == wiiform::ReturnTo::file) {
      require(!read_authenticated_file(nand, find_path(nand, menu + "/data/main.bin"), menu_app_limit).isEmpty(),
              "Priiloader file boot needs an installed executable");
    }
    if (settings.autoboot == Autoboot::homebrew_channel) {
      bool present = false;
      for (const auto tid : wiiform::homebrew_channel_title_ids) {
        const auto low = QString::number(static_cast<qulonglong>(tid), 16)
                             .rightJustified(8, QLatin1Char('0'));
        present |= find_path(nand, QString("/title/00010001/") + low + "/content/title.tmd") .has_value();
      }
      require(present, "Priiloader Homebrew Channel autoboot needs an installed Homebrew Channel");
    }
    if (settings.autoboot == Autoboot::bootmii_ios) {
      const auto bootmii = read_authenticated_file(
          nand, find_path(nand, "/title/00000001/000000fe/content/title.tmd"), tmd_limit);
      require(bootmii.size() >= 0x1e4 && qFromBigEndian<quint16>(bootmii.constData() + 0x1dc) == 31338,
              "Priiloader BootMii autoboot needs BootMii installed as IOS");
    }
  }

  struct PendingFile {
    QString path;
    QByteArray bytes;
    quint8 other_permissions = NAND_RW;
    bool replace = false;
    quint16 existing_id = 0;
  };

  auto clusters(qsizetype bytes) -> quint64 {
    return (quint64(bytes) + 0x3fff) / 0x4000;
  }

  void persist_buffer(NandBin& nand, const QMap<QString, PendingFile>& files) {
    for (auto it = files.cbegin(); it != files.cend(); ++it) {
      const auto& file = it.value();
      if (file.replace) require(nand.Delete(file.path), "Cannot replace a Priiloader target file");
      const auto id = file.existing_id ? file.existing_id :
          nand.CreateEntry(file.path, 0x1000, 1, NAND_FILE, NAND_RW, NAND_RW, file.other_permissions);
      require(id && nand.SetData(id, file.bytes), "Cannot write a Priiloader target file");
    }
    require(nand.WriteMetaData(), "Cannot persist Priiloader NAND metadata");
  }

  void verify_readback_buffer(std::span<std::uint8_t> image,
                              const QMap<QString, PendingFile>& files) {
    const auto layout = inspect::validate_layout(std::span<const std::uint8_t>(image.data(), image.size()), false);
    nand_buffer::Device device(std::span<const std::uint8_t>(image.data(), image.size()));
    NandBin nand;
    require(nand.SetDevice(&device) && nand.InitNand() && nand.CheckHmacMeta(layout.active_metadata),
            "Cannot authenticate the prepared Priiloader image");
    inspect::verify_filesystem(nand, false, layout.active_metadata);
    for (const auto& file : files)
      require(read_authenticated_file(nand, find_path(nand, file.path), file.bytes.size()) == file.bytes,
              "Priiloader file readback differs from the planned bytes");
    const auto evidence = inspect::priiloader_evidence(nand);
    require(evidence.layout == inspect::PriiloaderLayout::Installed,
            "Prepared image lacks the verified Priiloader layout");
  }

  void plan_hacks(NandBin& nand, int version,
                  std::span<const std::uint8_t> definitions,
                  std::span<const wiiform::Hack> enabled_hacks,
                  QMap<QString, PendingFile>& files) {
    const auto definition_entry = find_path(nand, priiloader::hack_definitions_path);
    const auto state_entry = find_path(nand, priiloader::hack_states_path);
    require(bool(definition_entry) == bool(state_entry),
            "Priiloader hack files must be a matching pair");
    const auto desired = priiloader::prepare_hacks(definitions, version, enabled_hacks);
    if (definition_entry) {
      const auto existing = read_authenticated_file(nand, definition_entry, 64 * 1024);
      require(existing == desired.definitions,
              "Existing Priiloader hack definitions differ from the pinned 0.10.0 release");
      if (read_authenticated_file(nand, state_entry, 64 * 1024) != desired.states)
        files["hack_states"] = {priiloader::hack_states_path, desired.states, NAND_RW, true};
    } else {
      files["hack_definitions"] = {priiloader::hack_definitions_path, desired.definitions};
      files["hack_states"] = {priiloader::hack_states_path, desired.states};
    }
  }

  void check_buffer_capacity(NandBin& nand, int orphan_entries,
                             const QMap<QString, PendingFile>& files) {
    int needed_entries = 0;
    quint64 needed_clusters = 0;
    for (const auto& file : files) {
      const auto existing = find_path(nand, file.path);
      const auto new_clusters = clusters(file.bytes.size());
      if (!existing) {
        ++needed_entries;
        needed_clusters += new_clusters;
      } else {
        // A replaced file is recreated before its old entry is reusable; one rewritten in place only grows.
        if (file.replace) ++needed_entries;
        const auto old_clusters = clusters(existing->size);
        if (new_clusters > old_clusters) needed_clusters += new_clusters - old_clusters;
      }
    }
    if (needed_entries || needed_clusters)
      working_image::require_capacity(nand, orphan_entries,
                                      needed_entries, needed_clusters);
  }

  void require_clean_menu_data(NandBin& nand) {
    const auto data = find_path(nand, menu + "/data");
    require(data && data->IsDir(), "System Menu data directory is missing");
    for (const auto* name : {"loader.ini", "hacks.ini", "hacks_s.ini", "hacksh_s.ini", "hackshas.ini", "password.txt"})
      require(!find_path(nand, menu + "/data/" + name), "System Menu contains existing Priiloader data");
  }

  void require_installed_menu(NandBin& nand,
                              const inspect::PriiloaderEvidence& evidence) {
    require(evidence.layout == inspect::PriiloaderLayout::Installed &&
                evidence.backup_tmd_nintendo_signed &&
                evidence.backup_app_matches_original_tmd_hash &&
                evidence.backup_app_matches_original_tmd_size,
            "Settings writes need a verified Priiloader 0.10.0 installation with its original System Menu");
    const auto app = read_authenticated_file(nand, find_path(nand, evidence.boot_app_path), release_app_size);
    require(priiloader::is_release_app(app),
            "Installed Priiloader app does not match the pinned release");
    const auto original = read_authenticated_file(nand, find_path(nand, original_tmd_path), tmd_limit);
    const auto active = read_authenticated_file(nand, find_path(nand, tmd_path), tmd_limit);
    require(active == patched_tmd(original, app), "Installed System Menu TMD differs from the pinned installer layout");
    const auto ticket = authentic_ticket(nand, original);
    require(read_authenticated_file(nand, find_path(nand, content + "ticket"), ticket.size()) == ticket,
            "Priiloader content ticket differs from the signed System Menu ticket");
  }
}

namespace priiloader {
  void apply(std::span<std::uint8_t> image, std::span<const std::uint8_t> app_bytes,
             std::span<const std::uint8_t> definition_bytes,
             const wiiform::PriiloaderSettings& settings,
             std::span<const wiiform::Hack> enabled_hacks) {
    require(app_bytes.empty() || app_bytes.size() == release_app_size,
            "Expected the extracted Priiloader 0.10.0 Wii app");
    const auto app = QByteArray::fromRawData(reinterpret_cast<const char*>(app_bytes.data()),
                                             static_cast<int>(app_bytes.size()));
    require(app.isEmpty() || priiloader::is_release_app(app),
            "Priiloader app SHA-256 does not match the pinned release");

    working_image::Image working(image);
    auto& nand = working.nand;
    inspect::verify_filesystem(nand, false, working.layout.active_metadata);
    const auto evidence = inspect::priiloader_evidence(nand);
    const bool installing = evidence.layout == inspect::PriiloaderLayout::StockBoot;
    require(installing || evidence.layout == inspect::PriiloaderLayout::Installed,
            "Priiloader preparation needs a stock System Menu or an existing Priiloader layout");
    const int version = menu_version(evidence);

    QMap<QString, PendingFile> files;
    if (installing) {
      require(!app.isEmpty(), "Installing Priiloader requires the pinned app payload");
      inspect::verify_stock_titles(
          nand, {{0x100000002ull, static_cast<quint16>(version)}, {0x100000050ull, 6944}});
      require_clean_menu_data(nand);
      const auto& app_path = evidence.boot_app_path;
      const auto& backup_path = evidence.backup_app_path;
      require(!backup_path.isEmpty(),
              "System Menu boot CID does not support the Wii Priiloader backup layout");
      for (const auto& path : {backup_path, original_tmd_path, content + "ticket"})
        require(!find_path(nand, path), "Priiloader installer backup or ticket already exists");
      const auto tmd = read_authenticated_file(nand, find_path(nand, tmd_path), tmd_limit);
      const auto original = read_authenticated_file(nand, find_path(nand, app_path), menu_app_limit);
      const auto ticket = authentic_ticket(nand, tmd);
      files["backup_app"] = {backup_path, original};
      files["backup_tmd"] = {original_tmd_path, tmd, 0};
      files["boot_app"] = {app_path, app, NAND_RW, true};
      files["ticket"] = {content + "ticket", ticket};
      files["tmd"] = {tmd_path, patched_tmd(tmd, app), 0, true};
    } else {
      require_installed_menu(nand, evidence);
    }

    const auto settings_entry = find_path(nand, settings_path);
    // Start from pinned 0.10.0 defaults on every application, replacing prior
    // settings deterministically, including fields hidden by the recipe API.
    const auto settings_bytes = encode_settings(settings);
    check_destinations(nand, settings);
    if (!settings_entry || read_authenticated_file(nand, settings_entry, 32) != settings_bytes) {
      files["settings"] = {settings_path, settings_bytes, NAND_RW, false,
                            settings_entry ? settings_entry->index : quint16(0)};
    }

    plan_hacks(nand, version, definition_bytes, enabled_hacks, files);

    check_buffer_capacity(nand, working.layout.orphan_entries, files);
    if (files.isEmpty()) return;
    persist_buffer(nand, files);
    verify_readback_buffer(image, files);
  }
}
