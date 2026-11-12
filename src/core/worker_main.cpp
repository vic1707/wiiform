// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/read.hpp>

#include <BuildOptions.hpp>
#include <Preflight.hpp>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryFile>

#include <iostream>
#include <stdexcept>
#include <unistd.h>

namespace {
  using wiiform::BootPolicy;
  using wiiform::SettingOverride;

  auto as_string(const QString& value) -> std::string { return value.toStdString(); }

  auto api_options(const nand_build::Options& old) -> std::vector<SettingOverride> {
    std::vector<SettingOverride> settings;
    settings.reserve(static_cast<std::size_t>(old.setting_overrides.size()));
    for (auto it = old.setting_overrides.cbegin(); it != old.setting_overrides.cend(); ++it)
      settings.push_back({as_string(it.key()), as_string(it.value())});
    return settings;
  }

  auto json_object(const std::string& bytes) -> QJsonObject {
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(QByteArray::fromStdString(bytes), &error);
    if (error.error != QJsonParseError::NoError || !document.isObject())
      throw std::runtime_error("Read API returned an invalid JSON report");
    return document.object();
  }

  void print_json(const std::string& bytes) {
    std::cout << bytes << '\n';
  }

  auto write_private_bytes(const QString& path, const QByteArray& data, const char* prefix) -> void {
    const QFileInfo destination(QDir::cleanPath(QFileInfo(path).absoluteFilePath()));
    if (path.isEmpty() || destination.exists() || destination.isSymLink() || !destination.absoluteDir().exists())
      throw std::runtime_error("Output needs a new path in an existing directory");
    QTemporaryFile staging(destination.absoluteDir().filePath(QString::fromLatin1(prefix) + "XXXXXX"));
    if (!staging.open() || !staging.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner) ||
        staging.write(data) != data.size() || !staging.flush() || fsync(staging.handle()) != 0 ||
        !staging.rename(destination.absoluteFilePath()))
      throw std::runtime_error("Cannot publish private read-only output");
    staging.setAutoRemove(false);
  }

  int print_inspection(const QString& path, wiiform::read::Inspection scope) {
    std::cout << wiiform::read::inspect(as_string(path), scope) << '\n';
    return 0;
  }

  int run(int argc, char** argv) {
    QCoreApplication app(argc, argv);
    auto args = app.arguments().mid(1);
    try {
      if (args.size() == 3 && args[0] == "--seed-export") {
        wiiform::read::export_seed(as_string(args[1]), as_string(args[2]));
        std::cout << "Wrote UNENCRYPTED output.\n";
        return 0;
      }
      if (args.size() == 3 && args[0] == "--seed-unpack") {
        wiiform::read::unpack_seed(as_string(args[1]), as_string(args[2]));
        std::cout << "Wrote UNENCRYPTED output.\n";
        return 0;
      }
      if (args.size() == 2) {
        if (args[0] == "--inspect-priiloader")
          return print_inspection(args[1], wiiform::read::Inspection::Priiloader);
        if (args[0] == "--inspect-display")
          return print_inspection(args[1], wiiform::read::Inspection::Display);
        if (args[0] == "--inspect-sysconf")
          return print_inspection(args[1], wiiform::read::Inspection::SystemSettings);
        if (args[0] == "--read-display-source") {
          const auto source = wiiform::read::display_source(as_string(args[1]));
          QJsonObject payload{
              {"sysconf", QString::fromLatin1(QByteArray(
                               reinterpret_cast<const char*>(source.sysconf.data()),
                               static_cast<int>(source.sysconf.size())).toBase64())},
              {"keys_data_sha256", QString::fromStdString(source.keys_data_sha256)},
              {"settings_sha256", QString::fromStdString(source.settings_sha256)}};
          std::cout << QJsonDocument(payload).toJson(QJsonDocument::Compact).toStdString() << '\n';
          return 0;
        }
      }
      if (args.size() == 3 && args[0] == "--export-priiloader-app") {
        const auto app_bytes = wiiform::read::priiloader_app(as_string(args[1]));
        const auto data = QByteArray(reinterpret_cast<const char*>(app_bytes.data()),
                                     static_cast<int>(app_bytes.size()));
        write_private_bytes(args[2], data, ".priiloader-app-");
        std::cout << "Exported the pinned Priiloader app.\n";
        return 0;
      }

      if (args.size() == 2 && args[0] == "--stock-titles") {
        QJsonArray rows;
        for (const auto& title : wiiform::read::stock_titles(as_string(args[1])))
          rows.append(QJsonArray{QString("%1").arg(title.title_id, 16, 16, QChar('0')), title.version});
        std::cout << QJsonDocument(rows).toJson(QJsonDocument::Compact).toStdString() << '\n';
        return 0;
      }

      if (!args.isEmpty() && args.first() == "--inspect-wad") {
        args.removeFirst();
        bool allow_unsigned = false;
        QString expected_sha256;
        while (!args.isEmpty() && args.first().startsWith("--")) {
          const auto flag = args.takeFirst();
          if (flag == "--allow-unverified-signatures" && !allow_unsigned) {
            allow_unsigned = true;
          } else if (flag == "--sha256" && expected_sha256.isEmpty() && !args.isEmpty()) {
            expected_sha256 = args.takeFirst();
          } else {
            throw std::runtime_error("Invalid WAD inspection options");
          }
        }
        if (args.size() != 1) throw std::runtime_error("WAD inspection needs one package path");
        const auto policy = allow_unsigned ? wiiform::WadPolicy::legacy_channel
                                           : wiiform::WadPolicy::standard;
        print_json(wiiform::read::inspect_wad(as_string(args[0]), as_string(expected_sha256), policy));
        return 0;
      }

      if (!args.isEmpty() && args.first() == "--inventory-json") {
        if (args.size() != 2) throw std::runtime_error("Inventory JSON needs one NAND path");
        print_json(wiiform::read::inventory(as_string(args[1])));
        return 0;
      }

      if (!args.isEmpty() && args.first() == "--preflight-json") {
        args.removeFirst();
        const auto request = nand_build::parse_arguments(args, true);
        const auto& paths = request.positionals;
        if (paths.size() != (request.candidate_only ? 2 : 3))
          throw std::runtime_error("Preflight JSON needs seed, optional reference, and candidate paths");
        std::vector<SettingOverride> settings;
        for (auto it = request.options.setting_overrides.cbegin();
             it != request.options.setting_overrides.cend(); ++it)
          settings.push_back({as_string(it.key()), as_string(it.value())});
        const auto policy = request.options.boot_policy == inspect::BootPolicy::Stock
                                ? wiiform::BootPolicy::stock
                                : wiiform::BootPolicy::preserve_source;
        print_json(wiiform::read::preflight(
            as_string(paths[0]), request.candidate_only ? std::string{} : as_string(paths[1]),
            as_string(paths.last()), policy, settings));
        return 0;
      }

      const bool inventory_command = !args.isEmpty() && args.first() == "--inventory";
      if (inventory_command) args.removeFirst();
      const auto request = inventory_command ? nand_build::Arguments{{}, args, false}
                                            : nand_build::parse_arguments(args, true);
      const auto& paths = request.positionals;
      if (paths.size() != (inventory_command ? 2 : request.candidate_only ? 3 : 4)) {
        std::cerr << "Use wii-nand preflight [BUILD_OPTIONS] SEED REFERENCE CANDIDATE NEW_REPORT.json\n"
                     "Use --candidate-only to omit REFERENCE.\n";
        return 2;
      }
      if (paths.contains(QString())) throw std::runtime_error("An input or output path is empty");
      const auto destination = paths.last();
      const QFileInfo output(destination);
      const auto output_path = output.absoluteDir().canonicalPath() + "/" + output.fileName();
      if (output.exists() || output.isSymLink() || !output.absoluteDir().exists())
        throw std::runtime_error("Report needs a new destination in an existing directory");
      for (const auto& path : paths.mid(0, paths.size() - 1)) {
        const QFileInfo input(path);
        if (input.absoluteDir().canonicalPath() + "/" + input.fileName() == output_path)
          throw std::runtime_error("Report destination must differ from every input");
      }

      QJsonObject report;
      int code = 0;
      if (inventory_command) {
        report = json_object(wiiform::read::inventory(as_string(paths[0])));
        code = report.value("status").toString() == "pass" ? 0 : 1;
      } else {
        const auto options = api_options(request.options);
        report = json_object(wiiform::read::preflight(
            as_string(paths[0]), request.candidate_only ? std::string{} : as_string(paths[1]),
            as_string(paths[paths.size() - 2]),
            request.options.boot_policy == inspect::BootPolicy::Stock ? BootPolicy::stock
                                                                      : BootPolicy::preserve_source,
            options));
        const auto status = report.value("host_status").toString();
        code = status == "pass" ? 0 : status == "blocked" ? 3 : 1;
      }
      preflight::write_report(report, destination);
      if (inventory_command) {
        std::cout << "Read-only filesystem inventory: " << report.value("status").toString().toStdString()
                  << ". Wrote private evidence report.\n";
      } else {
        std::cout << "Preflight host checks: " << report.value("host_status").toString().toStdString()
                  << ". Restore readiness: blocked. Wrote private evidence report.\n";
      }
      return code;
    } catch (const std::exception& error) {
      std::cerr << error.what() << '\n';
      return 1;
    }
  }
} // namespace

int main(int argc, char** argv) { return run(argc, argv); }
