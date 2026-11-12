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
#include <QDir>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QTreeWidgetItem>
#include <QtEndian>
#include <memory>
#include <stdexcept>
#include <sys/file.h>
#include <unistd.h>

namespace {
  constexpr qint64 nand_size = 0x21000400;
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

  auto sha256(const QByteArray& bytes) -> QByteArray {
    return QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex();
  }

  auto read_app(const QString& path) -> QByteArray {
    const QFileInfo info(path);
    QFile file(path);
    require(info.isFile() && !info.isSymLink() && file.open(QIODevice::ReadOnly) &&
                file.size() == release_app_size, "Expected the extracted Priiloader 0.10.0 Wii app");
    const auto bytes = file.read(release_app_size + 1);
    require(priiloader::is_release_app(bytes),
            "Priiloader app SHA-256 does not match the pinned release");
    return bytes;
  }

  template<class Edit>
  auto edit_copy(const QString& input, const QString& output, Edit edit) -> QJsonObject {
    const QFileInfo destination(QDir::cleanPath(QFileInfo(output).absoluteFilePath()));
    require(!output.isEmpty() && !destination.exists() && !destination.isSymLink() &&
                destination.absoluteDir().exists(), "Priiloader output needs a new path in an existing directory");
    const QFileInfo info(input);
    QFile original(input);
    require(info.isFile() && !info.isSymLink() && original.open(QIODevice::ReadOnly) &&
                original.size() == nand_size, "Priiloader needs a regular footer-bearing NAND image");
    require(flock(original.handle(), LOCK_SH | LOCK_NB) == 0, "Cannot lock the input NAND for copying");

    QTemporaryDir staging(destination.absoluteDir().filePath(".wii-priiloader-XXXXXX"));
    require(staging.isValid(), "Cannot create private Priiloader staging directory");
    const auto image = staging.filePath("nand.bin");
    {
      QFile copy(image);
      require(copy.open(QIODevice::WriteOnly | QIODevice::NewOnly) &&
                  copy.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner),
              "Cannot create a private NAND working copy");
      for (qint64 offset = 0; offset < nand_size;) {
        const auto bytes = original.read(qMin(qint64(1024 * 1024), nand_size - offset));
        require(!bytes.isEmpty() && copy.write(bytes) == bytes.size(), "Cannot copy the input NAND");
        offset += bytes.size();
      }
      require(original.atEnd() && copy.flush(), "Cannot finish the NAND working copy");
    }
    auto report = edit(image);
    QFile verified(image);
    require(verified.open(QIODevice::ReadOnly) && fsync(verified.handle()) == 0,
            "Cannot sync the verified Priiloader image");
    // Link publication fails if a concurrent caller creates the destination.
    require(link(QFile::encodeName(image).constData(), QFile::encodeName(destination.absoluteFilePath()).constData()) == 0,
            "Cannot publish Priiloader NAND to the new path");
    return report;
  }

  auto menu_version(const QJsonObject& evidence) -> int {
    const auto tmd = evidence["system_menu"].toObject()["tmd"].toObject();
    const int version = tmd["version"].toInt();
    require((version == 512 || version == 513 || version == 514) && tmd["ios"].toString() == "0000000100000050",
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

  void check_destinations(NandBin& nand, QTreeWidgetItem* tree, const QJsonObject& settings) {
    const auto values = settings["values"].toObject();
    const auto autoboot = values["autoboot"].toString();
    if (autoboot == "file" || values["return_to"].toString() == "file") {
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

  auto persist(NandBin& nand, const QMap<QString, PendingFile>& files) -> QJsonObject {
    QJsonObject report;
    for (auto it = files.cbegin(); it != files.cend(); ++it) {
      const auto& file = it.value();
      if (file.replace) require(nand.Delete(file.path), "Cannot replace a Priiloader target file");
      const auto id = file.existing_id ? file.existing_id :
          nand.CreateEntry(file.path, 0x1000, 1, NAND_FILE, NAND_RW, NAND_RW, file.other_permissions);
      require(id && nand.SetData(id, file.bytes), "Cannot write a Priiloader target file");
      report[it.key()] = QJsonObject{{"path", file.path}, {"size", static_cast<qint64>(file.bytes.size())},
                                     {"sha256", QString::fromLatin1(sha256(file.bytes))}};
    }
    require(nand.WriteMetaData(), "Cannot persist Priiloader NAND metadata");
    return report;
  }

  void verify_readback(const QString& image, const QMap<QString, PendingFile>& files) {
    const auto layout = inspect::validate_layout(image, false);
    NandBin nand;
    require(nand.SetPath(image) && nand.InitNand() && nand.CheckHmacMeta(layout.active_metadata),
            "Cannot authenticate the prepared Priiloader image");
    inspect::verify_filesystem(nand, false, layout.active_metadata);
    const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    for (const auto& file : files)
      require(read_authenticated_file(nand, find_path(tree.get(), file.path), file.bytes.size()) == file.bytes,
              "Priiloader file readback differs from the planned bytes");
    const auto report = inspect::priiloader_evidence(nand);
    require(report["status"].toString() == "priiloader-layout", "Prepared image lacks the verified Priiloader layout");
  }

  void require_clean_menu_data(QTreeWidgetItem* tree) {
    const auto* data = find_path(tree, menu + "/data");
    require(data && working_image::entry_kind(data) == NAND_DIR, "System Menu data directory is missing");
    for (const auto* name : {"loader.ini", "hacks.ini", "hacks_s.ini", "hacksh_s.ini", "hackshas.ini", "password.txt"})
      require(!find_path(tree, menu + "/data/" + name), "System Menu contains existing Priiloader data");
  }

  void require_installed_menu(NandBin& nand, QTreeWidgetItem* tree, const QJsonObject& evidence) {
    const auto boot = evidence["system_menu"].toObject();
    require(evidence["status"].toString() == "priiloader-layout" &&
                boot["backup_tmd"].toObject()["nintendo_signed"].toBool() &&
                boot["backup_app"].toObject()["matches_original_tmd_hash"].toBool() &&
                boot["backup_app"].toObject()["matches_original_tmd_size"].toBool(),
            "Settings writes need a verified Priiloader 0.10.0 installation with its original System Menu");
    const auto app_path = boot["boot_app"].toObject()["path"].toString();
    const auto app = read_authenticated_file(nand, find_path(tree, app_path), release_app_size);
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
  auto install(const QString& input, const QString& app, const QString& output,
               const QMap<QString, QString>& overrides) -> QJsonObject {
    const auto payload = read_app(app);
    const auto config = overrides.isEmpty() ? QByteArray() : patch_settings(default_settings(), overrides);
    return edit_copy(input, output, [&](const QString& image) {
      working_image::Image working(image);
      auto& nand = working.nand;
      inspect::verify_filesystem(nand, false, working.layout.active_metadata);
      const auto evidence = inspect::priiloader_evidence(nand);
      require(evidence["status"].toString() == "stock-boot", "Priiloader install needs an authenticated stock System Menu boot app");
      const auto version = menu_version(evidence);
      inspect::verify_stock_titles(nand, {{0x100000002ull, static_cast<quint16>(version)}, {0x100000050ull, 6944}});
      const auto boot = evidence["system_menu"].toObject();
      const auto app_path = boot["boot_app"].toObject()["path"].toString();
      const auto backup_path = boot["backup_app"].toObject()["path"].toString();
      require(!backup_path.isEmpty(), "System Menu boot CID does not support the Wii Priiloader backup layout");
      const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
      require_clean_menu_data(tree.get());
      for (const auto& path : {backup_path, original_tmd_path, content + "ticket"})
        require(!find_path(tree.get(), path), "Priiloader installer backup or ticket already exists");
      const auto tmd = read_authenticated_file(nand, find_path(tree.get(), tmd_path), tmd_limit);
      const auto original = read_authenticated_file(nand, find_path(tree.get(), app_path), menu_app_limit);
      const auto ticket = authentic_ticket(nand, tree.get(), tmd);
      QMap<QString, PendingFile> files{
          {"backup_app", {backup_path, original}},
          {"backup_tmd", {original_tmd_path, tmd, 0}},
          {"boot_app", {app_path, payload, NAND_RW, true}},
          {"ticket", {content + "ticket", ticket}},
          {"tmd", {tmd_path, patched_tmd(tmd, payload), 0, true}},
      };
      QJsonObject report{{"status", "installed"}, {"release", release}, {"menu_version", version},
                         {"app_sha256", QString::fromLatin1(release_app_hash)}};
      if (!config.isEmpty()) {
        const auto settings = inspect_settings(config);
        check_destinations(nand, tree.get(), settings);
        files["settings"] = {settings_path, config};
        report["settings"] = settings;
      }
      working_image::require_capacity(nand, tree->child(0), working.layout.orphan_entries, files.size() - 2,
                                      clusters(original.size()) + clusters(tmd.size()) + clusters(ticket.size()) +
                                          clusters(config.size()));
      report["files"] = persist(nand, files);
      verify_readback(image, files);
      return report;
    });
  }

  auto set_settings(const QString& input, const QString& output,
                    const QMap<QString, QString>& overrides,
                    const QString& pinned_definitions,
                    const QMap<QString, bool>& hack_overrides) -> QJsonObject {
    return edit_copy(input, output, [&](const QString& image) {
      working_image::Image working(image);
      auto& nand = working.nand;
      inspect::verify_filesystem(nand, false, working.layout.active_metadata);
      const auto evidence = inspect::priiloader_evidence(nand);
      const auto version = menu_version(evidence);
      const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
      require_installed_menu(nand, tree.get(), evidence);
      const auto* entry = find_path(tree.get(), settings_path);
      const auto original = entry ? read_authenticated_file(nand, entry, 32) : default_settings();
      const auto config = patch_settings(original, overrides);
      const auto settings = inspect_settings(config);
      check_destinations(nand, tree.get(), settings);
      QJsonObject report{{"status", "configured"}, {"release", release}, {"menu_version", version},
                         {"settings", settings}};
      QMap<QString, PendingFile> files;
      if (!entry || config != original)
        files["settings"] = {settings_path, config, NAND_RW, false, entry ? entry_id(entry) : quint16(0)};
      if (!hack_overrides.isEmpty()) {
        const auto* definitions = find_path(tree.get(), hack_definitions_path);
        const auto* states = find_path(tree.get(), hack_states_path);
        require(bool(definitions) == bool(states), "Priiloader hack files must be a matching pair");
        QByteArray previous_definitions;
        QByteArray previous_states;
        bool recognized_legacy = false;
        if (definitions) {
          previous_definitions = read_authenticated_file(nand, definitions, 64 * 1024);
          previous_states = read_authenticated_file(nand, states, 64 * 1024);
          recognized_legacy = previous_definitions == priiloader::legacy_fix_480p_definitions();
          if (recognized_legacy) {
            require(previous_states.size() == 1 && (previous_states.at(0) == 0 || previous_states.at(0) == 1),
                    "Invalid legacy Priiloader hack state");
          } else if (pinned_definitions.isEmpty()) {
            require(false, "Existing Priiloader hack definitions need the pinned 0.10.0 source");
          }
        }
        HackFiles desired;
        if (pinned_definitions.isEmpty()) {
          require(hack_overrides.size() == 1 && hack_overrides.contains("480p graphics fix in system menu"),
                  "Priiloader hacks need the pinned hacks_hash.ini source");
          desired.definitions = priiloader::legacy_fix_480p_definitions();
          desired.states = QByteArray(1, hack_overrides.value("480p graphics fix in system menu") ? char(1) : char(0));
          desired.selected["fix_480p"] = hack_overrides.value("480p graphics fix in system menu");
        } else {
          auto requested = hack_overrides;
          if (recognized_legacy && !requested.contains("480p graphics fix in system menu"))
            requested["480p graphics fix in system menu"] = previous_states.at(0) == 1;
          desired = prepare_hacks(pinned_definitions, version, requested,
                                  !definitions || recognized_legacy ? QByteArray{} : previous_states);
          if (recognized_legacy && !hack_overrides.contains("480p graphics fix in system menu"))
            desired.selected.remove("480p graphics fix in system menu");
        }
        if (definitions) {
          require(previous_definitions == desired.definitions || recognized_legacy,
                  "Existing Priiloader hack definitions differ from the pinned 0.10.0 release");
          if (previous_definitions != desired.definitions)
            files["hack_definitions"] = {priiloader::hack_definitions_path, desired.definitions, NAND_RW, true};
          if (previous_states != desired.states)
            files["hack_states"] = {hack_states_path, desired.states, NAND_RW, true};
        } else {
          files["hack_definitions"] = {priiloader::hack_definitions_path, desired.definitions};
          files["hack_states"] = {priiloader::hack_states_path, desired.states};
        }
        report["hacks"] = desired.selected;
      }
      report["changed"] = !files.isEmpty();
      if (files.isEmpty()) return report;

      int needed_entries = 0;
      quint64 needed_clusters = 0;
      for (const auto& file : files) {
        const auto* existing = find_path(tree.get(), file.path);
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
        working_image::require_capacity(nand, tree->child(0), working.layout.orphan_entries,
                                        needed_entries, needed_clusters);
      report["files"] = persist(nand, files);
      verify_readback(image, files);
      return report;
    });
  }
}
