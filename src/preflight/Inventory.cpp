// SPDX-License-Identifier: GPL-2.0-only
#include <Inventory.hpp>
#include <Display.hpp>
#include <Inspection.hpp>
#include <Layout.hpp>
#include <PriiloaderRelease.hpp>
#include <SysConf.hpp>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QtEndian>
#include <functional>
#include <memory>
#include <stdexcept>

#ifndef WIICLI_VERSION
#define WIICLI_VERSION "unknown"
#endif
#ifndef WIICLI_REVISION
#define WIICLI_REVISION "unknown"
#endif

namespace {
  constexpr qint64 nand_size = 0x21000000;
  constexpr int boot_size = 0x108000;

  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  struct QuietQt {
    QtMessageHandler previous = qInstallMessageHandler([](QtMsgType, const QMessageLogContext&, const QString&) {});
    ~QuietQt() { qInstallMessageHandler(previous); }
  };

  auto hash(const QByteArray& data) -> QString {
    return QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex());
  }

  auto digest(const QString& path) -> QString {
    QFile file(path);
    require(QFileInfo(path).isFile() && !QFileInfo(path).isSymLink() && file.open(QIODevice::ReadOnly),
            "Cannot read inventory input");
    QCryptographicHash sha(QCryptographicHash::Sha256);
    require(sha.addData(&file) && file.error() == QFileDevice::NoError, "Cannot hash inventory input");
    return QString::fromLatin1(sha.result().toHex());
  }
}

namespace inventory {
  auto inspect(const QString& path, Scope scope) -> QJsonObject {
    QuietQt quiet;
    QJsonObject report{{"schema_version", 1}, {"kind", "nand-inventory"}, {"status", "fail"},
                       {"created_utc", QDateTime::currentDateTimeUtc().toString(Qt::ISODate)},
                       {"restore_status", "blocked"},
                       {"scope", "console-inputs-and-live-filesystem"}};
    QJsonArray checks;
    QMap<QString, QString> observed;
    bool failed = false;
    auto attempt = [&](const QString& id, bool ready, const std::function<void()>& operation) {
      if (!ready) {
        checks.append(QJsonObject{{"id", id}, {"status", "blocked"},
                                  {"detail", "Required prerequisite did not pass"}});
        return false;
      }
      try {
        operation();
        checks.append(QJsonObject{{"id", id}, {"status", "pass"}, {"detail", "Check completed"}});
        return true;
      } catch (const std::exception& error) {
        failed = true;
        checks.append(QJsonObject{{"id", id}, {"status", "fail"},
                                  {"detail", QString::fromUtf8(error.what())}});
      } catch (...) {
        failed = true;
        checks.append(QJsonObject{{"id", id}, {"status", "fail"}, {"detail", "Inventory check failed"}});
      }
      return false;
    };

    QByteArray keys, boots;
    const auto loaded = attempt("input", true, [&] {
      const QFileInfo info(path);
      QFile file(path);
      require(info.isFile() && !info.isSymLink() && file.open(QIODevice::ReadOnly), "Cannot read NAND input");
      const auto size = file.size();
      require(size == nand_size || size == nand_size + 0x400, "Expected a full NAND dump with spare data");
      observed[path] = digest(path);
      boots = file.read(boot_size);
      require(boots.size() == boot_size, "Cannot read NAND boot area");
      QString key_source;
      if (size == nand_size + 0x400) {
        require(file.seek(nand_size), "Cannot read NAND key footer");
        keys = file.read(0x400);
        require(keys.size() == 0x400, "Truncated NAND key footer");
        key_source = "footer";
      }
      const auto sidecar_path = info.absoluteDir().filePath("keys.bin");
      const QFileInfo sidecar_info(sidecar_path);
      if (sidecar_info.exists() || sidecar_info.isSymLink()) {
        QFile sidecar(sidecar_path);
        require(sidecar_info.isFile() && !sidecar_info.isSymLink() && sidecar.open(QIODevice::ReadOnly) &&
                    sidecar.size() == 0x400, "Invalid sidecar keys file");
        observed[sidecar_path] = digest(sidecar_path);
        const auto sidecar_keys = sidecar.read(0x401);
        require(sidecar_keys.size() == 0x400 && sidecar.error() == QFileDevice::NoError,
                "Cannot read sidecar keys");
        require(keys.isEmpty() || keys == sidecar_keys, "Footer and sidecar keys disagree");
        key_source = keys.isEmpty() ? "sidecar" : "footer-and-sidecar";
        keys = sidecar_keys;
      }
      require(keys.size() == 0x400, "NAND input needs matching footer or sidecar keys");
      report["image"] = QJsonObject{{"size", double(size)}, {"sha256", observed[path]}, {"key_source", key_source}};
      report["identity"] = QJsonObject{{"boot_sha256", hash(boots)}, {"keys_sha256", hash(keys)}};
    });

    ::inspect::LayoutSummary layout;
    const auto bounded = attempt("layout", loaded, [&] {
      layout = ::inspect::validate_layout(path, false);
      report["layout"] = QJsonObject{{"active_metadata", layout.active_metadata},
                                     {"unowned_clusters", layout.unowned_clusters},
                                     {"reserved_clusters", layout.reserved_clusters},
                                     {"orphan_entries", layout.orphan_entries}};
    });
    std::unique_ptr<NandBin> nand;
    const auto authenticated = attempt("authentication", bounded, [&] {
      nand = std::make_unique<NandBin>();
      require(nand->SetPath(path) && nand->InitNand(), "Cannot read NAND filesystem");
      require(nand->Keys() == keys, "Filesystem decoder selected different keys");
      require(nand->CheckHmacMeta(layout.active_metadata), "Selected filesystem metadata does not authenticate");
    });
    const auto intact = attempt("filesystem_integrity", authenticated, [&] {
      const auto summary = ::inspect::verify_filesystem(*nand, false, layout.active_metadata);
      report["integrity"] = QJsonObject{{"checked_clusters", summary.checked_clusters},
                                        {"checked_files", summary.checked_files},
                                        {"authenticated_metadata_copies", summary.authenticated_metadata_copies},
                                        {"boot_ecc", "not-assessed"}, {"boot_signatures", "not-assessed"},
                                        {"title_signatures", "not-assessed"}, {"physical_health", "not-assessed"}};
    });
    const bool display = scope == Scope::Display || scope == Scope::DisplaySource;
    const bool inspect_priiloader = scope == Scope::Priiloader || scope == Scope::PriiloaderApp;
    const auto check_name = display ? "display" : inspect_priiloader ? "priiloader" : "inventory";
    const auto indexed = attempt(check_name, intact, [&] {
      if (inspect_priiloader) {
        auto evidence = ::inspect::priiloader_evidence(*nand);
        evidence["image_sha256"] = observed.value(path);
        if (scope == Scope::PriiloaderApp) {
          require(evidence["status"].toString() == "priiloader-layout",
                  "NAND does not have the recognized Wii Priiloader boot layout");
          const auto path = evidence["system_menu"].toObject()["boot_app"].toObject()["path"].toString();
          const auto app = ::inspect::read_file(*nand, path, priiloader::release_app_size);
          require(priiloader::is_release_app(app), "Installed Priiloader app does not match the pinned release");
          report["priiloader_app"] = QString::fromLatin1(app.toBase64());
        }
        report["priiloader"] = evidence;
        return;
      }
      if (display) {
        const auto data = ::inspect::read_file(*nand, sysconf::path, sysconf::size);
        report["display"] = sysconf::display_report(sysconf::read_display(data));
        if (scope == Scope::Display)
          report["system_settings"] = sysconf::settings_report(sysconf::read_settings(data));
        if (scope == Scope::DisplaySource) {
          const auto settings = ::inspect::read_file(*nand, "/title/00000001/00000002/data/setting.txt", 256);
          require(settings.size() == 256, "Invalid reference console settings");
          report["display_source"] = QJsonObject{{"sysconf", QString::fromLatin1(data.toBase64())},
                                                 {"keys_data_sha256", hash(keys.mid(sysconf::key_header_bytes))},
                                                 {"settings_sha256", hash(settings)}};
        }
        return;
      }
      const auto files = ::inspect::inventory_filesystem(*nand);
      for (auto it = files.begin(); it != files.end(); ++it) report[it.key()] = it.value();
      const auto fats = nand->GetFats();
      require(fats.size() == 0x8000, "Invalid inventory allocation table");
      QByteArray fat_bytes(0x10000, '\0');
      QJsonArray bad;
      for (int i = 0; i < fats.size(); ++i) {
        qToBigEndian<quint16>(fats[i], reinterpret_cast<uchar*>(fat_bytes.data() + i * 2));
        if (i >= 0x40 && fats[i] == 0xfffd) bad.append(i);
      }
      auto summary = report["layout"].toObject();
      summary["fat_sha256"] = hash(fat_bytes);
      summary["bad_clusters"] = bad;
      QFile file(path);
      require(file.open(QIODevice::ReadOnly) && file.seek(qint64(layout.active_metadata) * 0x4200 + 4),
              "Cannot read metadata generation");
      const auto generation = file.read(4);
      require(generation.size() == 4, "Cannot read metadata generation");
      summary["metadata_generation"] = double(qFromBigEndian<quint32>(
          reinterpret_cast<const uchar*>(generation.constData())));
      report["layout"] = summary;
    });
    const auto unchanged = attempt("inputs_unchanged", loaded, [&] {
      for (auto it = observed.cbegin(); it != observed.cend(); ++it)
        require(digest(it.key()) == it.value(), "An inventory input changed while checks were running");
    });
    QJsonObject tool{{"version", WIICLI_VERSION}, {"source_revision", WIICLI_REVISION}};
    try { tool["executable_sha256"] = digest(QCoreApplication::applicationFilePath()); }
    catch (...) { tool["executable_sha256"] = QJsonValue(); }
    report["tool"] = tool;
    report["checks"] = checks;
    report["status"] = !failed && indexed && unchanged ? "pass" : "fail";
    return report;
  }
}
