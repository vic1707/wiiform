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
#include <QTreeWidgetItem>
#include <QtEndian>
#include <memory>
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

  using working_image::entry_id;
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

  auto authentic_ticket(NandBin& nand, QTreeWidgetItem* tree, const QByteArray& tmd) -> QByteArray {
    const auto ticket = read_authenticated_file(nand, find_path(tree, ticket_path), 0x2a4);
    require(ticket.size() == 0x2a4 && check_title_metadata(tmd, ticket) &&
                check_cert_chain(ticket) == ERROR_SUCCESS && Ticket(ticket, false).Tid() == 0x100000002ull,
            "System Menu ticket is not an authentic stock ticket");
    return ticket;
  }

  void check_destinations(NandBin& nand, QTreeWidgetItem* tree, const priiloader::Settings& settings) {
    const auto autoboot = settings.autoboot;
    const auto return_to = settings.return_to;
    if (autoboot == "file" || return_to == "file") {
      require(!read_authenticated_file(nand, find_path(tree, menu + "/data/main.bin"), menu_app_limit).isEmpty(),
              "Priiloader file boot needs an installed executable");
    }
    if (autoboot == "homebrew_channel") {
      bool present = false;
      for (const auto* tid : {"48415858", "4a4f4449", "af1bf516", "4c554c5a", "4f484243"})
        present |= find_path(tree, QString("/title/00010001/") + tid + "/content/title.tmd") != nullptr;
      require(present, "Priiloader Homebrew Channel autoboot needs an installed Homebrew Channel");
    }
    if (autoboot == "bootmii_ios") {
      const auto bootmii = read_authenticated_file(
          nand, find_path(tree, "/title/00000001/000000fe/content/title.tmd"), tmd_limit);
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
    const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    for (const auto& file : files)
      require(read_authenticated_file(nand, find_path(tree.get(), file.path), file.bytes.size()) == file.bytes,
              "Priiloader file readback differs from the planned bytes");
    const auto evidence = inspect::priiloader_evidence(nand);
    require(evidence.layout == inspect::PriiloaderLayout::Installed,
            "Prepared image lacks the verified Priiloader layout");
  }

  void plan_hacks(NandBin& nand, QTreeWidgetItem* tree, int version,
                  std::span<const std::uint8_t> definitions,
                  const QMap<QString, bool>& requested,
                  QMap<QString, PendingFile>& files) {
    if (requested.isEmpty()) return;
    const auto* definition_entry = find_path(tree, priiloader::hack_definitions_path);
    const auto* state_entry = find_path(tree, priiloader::hack_states_path);
    require(bool(definition_entry) == bool(state_entry),
            "Priiloader hack files must be a matching pair");
    QByteArray previous_definitions;
    QByteArray previous_states;
    bool recognized_legacy = false;
    if (definition_entry) {
      previous_definitions = read_authenticated_file(nand, definition_entry, 64 * 1024);
      previous_states = read_authenticated_file(nand, state_entry, 64 * 1024);
      recognized_legacy = previous_definitions == priiloader::legacy_fix_480p_definitions();
      if (recognized_legacy) {
        require(previous_states.size() == 1 &&
                    (previous_states.at(0) == 0 || previous_states.at(0) == 1),
                "Invalid legacy Priiloader hack state");
      } else {
        require(!definitions.empty(),
                "Existing Priiloader hack definitions need the pinned 0.10.0 source");
      }
    }

    require(!definitions.empty(), "Priiloader hacks need pinned hacks_hash.ini bytes");
    auto overrides = requested;
    if (recognized_legacy && !overrides.contains("480p graphics fix in system menu"))
      overrides["480p graphics fix in system menu"] = previous_states.at(0) == 1;
    auto desired = priiloader::prepare_hacks(definitions, version, overrides,
                                             !definition_entry || recognized_legacy ? QByteArray{} : previous_states);
    if (definition_entry) {
      require(previous_definitions == desired.definitions || recognized_legacy,
              "Existing Priiloader hack definitions differ from the pinned 0.10.0 release");
      if (previous_definitions != desired.definitions)
        files["hack_definitions"] = {priiloader::hack_definitions_path, desired.definitions, NAND_RW, true};
      if (previous_states != desired.states)
        files["hack_states"] = {priiloader::hack_states_path, desired.states, NAND_RW, true};
    } else {
      files["hack_definitions"] = {priiloader::hack_definitions_path, desired.definitions};
      files["hack_states"] = {priiloader::hack_states_path, desired.states};
    }
  }

  void check_buffer_capacity(NandBin& nand, QTreeWidgetItem* tree, int orphan_entries,
                             const QMap<QString, PendingFile>& files) {
    int needed_entries = 0;
    quint64 needed_clusters = 0;
    for (const auto& file : files) {
      const auto* existing = find_path(tree, file.path);
      const auto new_clusters = clusters(file.bytes.size());
      if (!existing) {
        ++needed_entries;
        needed_clusters += new_clusters;
      } else if (file.replace) {
        ++needed_entries;
        bool size_ok = false;
        const auto old_size = existing->text(2).toULongLong(&size_ok, 16);
        require(size_ok, "Invalid Priiloader configuration file size");
        const auto old_clusters = clusters(old_size);
        if (new_clusters > old_clusters) needed_clusters += new_clusters - old_clusters;
      }
    }
    if (needed_entries || needed_clusters)
      working_image::require_capacity(nand, tree->child(0), orphan_entries,
                                      needed_entries, needed_clusters);
  }

  void require_clean_menu_data(QTreeWidgetItem* tree) {
    const auto* data = find_path(tree, menu + "/data");
    require(data && working_image::entry_kind(data) == NAND_DIR, "System Menu data directory is missing");
    for (const auto* name : {"loader.ini", "hacks.ini", "hacks_s.ini", "hacksh_s.ini", "hackshas.ini", "password.txt"})
      require(!find_path(tree, menu + "/data/" + name), "System Menu contains existing Priiloader data");
  }

  void require_installed_menu(NandBin& nand, QTreeWidgetItem* tree,
                              const inspect::PriiloaderEvidence& evidence) {
    require(evidence.layout == inspect::PriiloaderLayout::Installed &&
                evidence.backup_tmd_nintendo_signed &&
                evidence.backup_app_matches_original_tmd_hash &&
                evidence.backup_app_matches_original_tmd_size,
            "Settings writes need a verified Priiloader 0.10.0 installation with its original System Menu");
    const auto app = read_authenticated_file(nand, find_path(tree, evidence.boot_app_path), release_app_size);
    require(priiloader::is_release_app(app),
            "Installed Priiloader app does not match the pinned release");
    const auto original = read_authenticated_file(nand, find_path(tree, original_tmd_path), tmd_limit);
    const auto active = read_authenticated_file(nand, find_path(tree, tmd_path), tmd_limit);
    require(active == patched_tmd(original, app), "Installed System Menu TMD differs from the pinned installer layout");
    const auto ticket = authentic_ticket(nand, tree, original);
    require(read_authenticated_file(nand, find_path(tree, content + "ticket"), ticket.size()) == ticket,
            "Priiloader content ticket differs from the signed System Menu ticket");
  }
}

namespace priiloader {
  void apply(std::span<std::uint8_t> image, std::span<const std::uint8_t> app_bytes,
             std::span<const std::uint8_t> definition_bytes,
             const QMap<QString, QString>& setting_overrides,
             const QMap<QString, bool>& hack_overrides) {
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
    const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());

    QMap<QString, PendingFile> files;
    if (installing) {
      require(!app.isEmpty(), "Installing Priiloader requires the pinned app payload");
      inspect::verify_stock_titles(
          nand, {{0x100000002ull, static_cast<quint16>(version)}, {0x100000050ull, 6944}});
      require_clean_menu_data(tree.get());
      const auto& app_path = evidence.boot_app_path;
      const auto& backup_path = evidence.backup_app_path;
      require(!backup_path.isEmpty(),
              "System Menu boot CID does not support the Wii Priiloader backup layout");
      for (const auto& path : {backup_path, original_tmd_path, content + "ticket"})
        require(!find_path(tree.get(), path), "Priiloader installer backup or ticket already exists");
      const auto tmd = read_authenticated_file(nand, find_path(tree.get(), tmd_path), tmd_limit);
      const auto original = read_authenticated_file(nand, find_path(tree.get(), app_path), menu_app_limit);
      const auto ticket = authentic_ticket(nand, tree.get(), tmd);
      files["backup_app"] = {backup_path, original};
      files["backup_tmd"] = {original_tmd_path, tmd, 0};
      files["boot_app"] = {app_path, app, NAND_RW, true};
      files["ticket"] = {content + "ticket", ticket};
      files["tmd"] = {tmd_path, patched_tmd(tmd, app), 0, true};
    } else {
      require_installed_menu(nand, tree.get(), evidence);
      require(app.isEmpty(), "Priiloader is already installed, so no installer app is needed");
    }

    const auto* settings_entry = find_path(tree.get(), settings_path);
    const auto settings_before = settings_entry
        ? read_authenticated_file(nand, settings_entry, 32)
        : default_settings();
    const auto settings_bytes = patch_settings(settings_before, setting_overrides);
    const auto settings = read_settings(settings_bytes);
    check_destinations(nand, tree.get(), settings);
    if ((!installing || !setting_overrides.isEmpty()) &&
        (!settings_entry || settings_bytes != settings_before)) {
      files["settings"] = {settings_path, settings_bytes, NAND_RW, false,
                            settings_entry ? entry_id(settings_entry) : quint16(0)};
    }

    if (!hack_overrides.isEmpty()) {
      plan_hacks(nand, tree.get(), version, definition_bytes, hack_overrides, files);
    }

    check_buffer_capacity(nand, tree.get(), working.layout.orphan_entries, files);
    if (files.isEmpty()) return;
    persist_buffer(nand, files);
    verify_readback_buffer(image, files);
  }
}
