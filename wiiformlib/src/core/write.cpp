// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/write.hpp>
#include "QtRuntime.hpp"

#include <BuildOptions.hpp>
#include <Display.hpp>
#include <Install.hpp>
#include <Priiloader.hpp>
#include <Remove.hpp>

#include <QJsonDocument>
#include <QJsonObject>
#include <QMap>

#include <stdexcept>

namespace wiiform::core {
namespace {
  void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
  }

  auto qstring(const std::string& value, const char* label) -> QString {
    const auto converted = QString::fromUtf8(value.data(), qsizetype(value.size()));
    require(converted.toUtf8() == QByteArray(value.data(), qsizetype(value.size())), label);
    return converted;
  }

  auto json(const QJsonObject& object) -> std::string {
    return QJsonDocument(object).toJson(QJsonDocument::Compact).toStdString();
  }

  auto setting_map(const std::vector<SettingOverride>& values, const char* duplicate_error)
      -> QMap<QString, QString> {
    QMap<QString, QString> result;
    for (const auto& value : values) {
      const auto key = qstring(value.key, "Setting name must be UTF-8");
      const auto data = qstring(value.value, "Setting value must be UTF-8");
      require(!key.isEmpty() && !data.isEmpty(), "Setting name and value must not be empty");
      require(!result.contains(key), duplicate_error);
      result.insert(key, data);
    }
    return result;
  }

  auto display_overrides(const std::vector<SettingOverride>& values, bool setup_options)
      -> sysconf::SettingsOverrides {
    sysconf::SettingsOverrides result;
    for (const auto& value : values) {
      const auto key = qstring(value.key, "Setting name must be UTF-8");
      const auto data = qstring(value.value, "Setting value must be UTF-8");
      if (key == "aspect" && !result.display.widescreen && (data == "4:3" || data == "16:9"))
        result.display.widescreen = data == "16:9";
      else if (key == "progressive" && !result.display.progressive_scan && (data == "on" || data == "off"))
        result.display.progressive_scan = data == "on";
      else if (key == "pal60" && !result.display.pal60 && (data == "on" || data == "off"))
        result.display.pal60 = data == "on";
      else if (setup_options && key == "require-setup" && !result.require_setup && data == "true")
        result.require_setup = true;
      else if (setup_options && key == "nickname" && !result.nickname && !data.isEmpty())
        result.nickname = data;
      else if (setup_options && key == "language" && !result.language && !data.isEmpty())
        result.language = data.toLower();
      else
        throw std::runtime_error("Invalid or repeated system setting override");
    }
    return result;
  }

  auto sysconf_source(const DisplaySource& source) -> std::optional<sysconf::Source> {
    if (source.sysconf.empty()) {
      require(source.keys_data_sha256.empty() && source.settings_sha256.empty(),
              "Incomplete authenticated display source");
      return std::nullopt;
    }
    require(source.sysconf.size() == size_t(sysconf::size) &&
                source.keys_data_sha256.size() == 64 && source.settings_sha256.size() == 64,
            "Incomplete authenticated display source");
    return sysconf::Source{
        QByteArray(reinterpret_cast<const char*>(source.sysconf.data()), qsizetype(source.sysconf.size())),
        QByteArray(source.keys_data_sha256.data(), qsizetype(source.keys_data_sha256.size())),
        QByteArray(source.settings_sha256.data(), qsizetype(source.settings_sha256.size()))};
  }
}

void create(const std::string& seed, const std::string& output, BootPolicy boot_policy,
            const std::vector<SettingOverride>& setting_overrides) {
  detail::ensure_qcore_application();
  nand_build::Options options;
  switch (boot_policy) {
    case BootPolicy::stock: options.boot_policy = inspect::BootPolicy::Stock; break;
    case BootPolicy::preserve_source: options.boot_policy = inspect::BootPolicy::PreserveSource; break;
    default: throw std::runtime_error("Unsupported boot policy");
  }
  options.setting_overrides = setting_map(setting_overrides, "Duplicate system setting override");
  nand_build::create(qstring(seed, "Seed path must be UTF-8"),
                     qstring(output, "Output path must be UTF-8"), options);
}

void install_nus_title(const std::string& image, const std::string& cache,
                       std::uint64_t title_id, std::uint16_t version, bool offline) {
  detail::ensure_qcore_application();
  require(title_id != 0 && version != 0xffff,
          "Install requires a nonzero title ID and a concrete title version");
  title_install::install_nus_title(qstring(image, "Image path must be UTF-8"),
      qstring(cache, "Cache path must be UTF-8"), quint64(title_id), quint16(version), offline);
}

void install_wad(const std::string& image, const std::string& package,
                 const std::string& expected_sha256, WadPolicy policy) {
  detail::ensure_qcore_application();
  const auto sha = qstring(expected_sha256, "WAD SHA-256 must be UTF-8").toLatin1();
  require(!sha.isEmpty(), "WAD installation requires an expected SHA-256");
  require(policy == WadPolicy::standard || policy == WadPolicy::legacy_channel,
          "Unsupported WAD signature policy");
  const auto path = qstring(package, "WAD path must be UTF-8");
  require(path != "-", "WAD installation requires a file path");
  title_install::install_wad(qstring(image, "Image path must be UTF-8"), path, sha,
      policy == WadPolicy::legacy_channel);
}

void remove_title(const std::string& image, std::uint64_t title_id) {
  detail::ensure_qcore_application();
  title_remove::remove_channel(qstring(image, "Image path must be UTF-8"), quint64(title_id));
}

std::string set_display(const std::string& image, const DisplaySource& source,
                        const std::vector<SettingOverride>& overrides) {
  detail::ensure_qcore_application();
  const auto values = display_overrides(overrides, false);
  require(values.display.widescreen || values.display.progressive_scan || values.display.pal60 ||
              !source.sysconf.empty(),
          "No display settings were requested");
  return json(sysconf::set_display(qstring(image, "Image path must be UTF-8"), values.display,
                                   sysconf_source(source)));
}

std::string set_sysconf(const std::string& image, const DisplaySource& source,
                        const std::vector<SettingOverride>& overrides) {
  detail::ensure_qcore_application();
  const auto values = display_overrides(overrides, true);
  return json(sysconf::set_settings(qstring(image, "Image path must be UTF-8"), values,
                                    sysconf_source(source)));
}

std::string install_priiloader(const std::string& input_image, const std::string& app,
                               const std::string& output_image,
                               const std::vector<SettingOverride>& settings) {
  detail::ensure_qcore_application();
  return json(priiloader::install(qstring(input_image, "Input NAND path must be UTF-8"),
      qstring(app, "Priiloader app path must be UTF-8"),
      qstring(output_image, "Output NAND path must be UTF-8"),
      setting_map(settings, "Duplicate Priiloader setting override")));
}

std::string set_priiloader_settings(const std::string& input_image, const std::string& output_image,
                                   const std::vector<SettingOverride>& settings,
                                   const std::string& hack_definitions,
                                   const std::vector<SettingOverride>& hacks) {
  detail::ensure_qcore_application();
  QMap<QString, bool> hack_values;
  for (const auto& hack : hacks) {
    const auto key = qstring(hack.key, "Priiloader hack name must be UTF-8");
    const auto value = qstring(hack.value, "Priiloader hack state must be UTF-8");
    require(!key.isEmpty() && (value == "on" || value == "off") && !hack_values.contains(key),
            "Invalid or repeated Priiloader hack override");
    hack_values.insert(key, value == "on");
  }
  return json(priiloader::set_settings(qstring(input_image, "Input NAND path must be UTF-8"),
      qstring(output_image, "Output NAND path must be UTF-8"),
      setting_map(settings, "Duplicate Priiloader setting override"),
      hack_definitions.empty() ? QString{} : qstring(hack_definitions, "Hack definitions path must be UTF-8"),
      hack_values));
}
}
