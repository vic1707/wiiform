// SPDX-License-Identifier: GPL-2.0-only
#include <wiiform/read.hpp>

#include "QtRuntime.hpp"

#include <BuildOptions.hpp>
#include <Factory.hpp>
#include <Inventory.hpp>
#include <Preflight.hpp>
#include <PriiloaderRelease.hpp>
#include <Seed.hpp>
#include <SysConf.hpp>
#include <WadPackage.hpp>
#include <tiktmd.h>

#include <QByteArray>
#include <QCryptographicHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QtEndian>

#include <limits>
#include <stdexcept>

namespace {
  void ensure_qt_application() {
    (void)::wiiform::core::detail::ensure_qcore_application();
  }

  auto text(std::string_view value) -> QString {
    if (value.size() > static_cast<std::size_t>(std::numeric_limits<int>::max()))
      throw std::runtime_error("Path or value exceeds supported size");
    if (value.find('\0') != std::string_view::npos)
      throw std::runtime_error("Path or value contains a NUL byte");
    const auto result = QString::fromUtf8(value.data(), static_cast<int>(value.size()));
    if (result.toUtf8() != QByteArray(value.data(), static_cast<int>(value.size())))
      throw std::runtime_error("Path or value is not valid UTF-8");
    return result;
  }

  auto bytes(const QByteArray& value) -> std::vector<std::uint8_t> {
    if (value.isEmpty()) return {};
    const auto* begin = reinterpret_cast<const std::uint8_t*>(value.constData());
    return {begin, begin + value.size()};
  }

  auto json(const QJsonObject& object) -> std::string {
    return QJsonDocument(object).toJson(QJsonDocument::Compact).toStdString();
  }

  auto passed_report(const QString& path, ::inventory::Scope scope) -> QJsonObject {
    const auto report = ::inventory::inspect(path, scope);
    if (report.value("status").toString() == "pass") return report;
    const auto checks = report.value("checks").toArray();
    for (const auto& value : checks) {
      const auto check = value.toObject();
      if (check.value("status").toString() == "fail")
        throw std::runtime_error(check.value("detail").toString().toStdString());
    }
    throw std::runtime_error("Cannot complete the read-only NAND inspection");
  }

  auto digest(const QByteArray& value) -> QString {
    return QString::fromLatin1(QCryptographicHash::hash(value, QCryptographicHash::Sha256).toHex());
  }
} // namespace

namespace wiiform::read {

  void export_seed(std::string_view nand_backup, std::string_view new_seed) {
    ensure_qt_application();
    seed::export_backup(text(nand_backup), text(new_seed));
  }

  void unpack_seed(std::string_view seed_path, std::string_view new_directory) {
    ensure_qt_application();
    (void)seed::unpack(text(seed_path), text(new_directory));
  }

  auto inventory(std::string_view nand) -> std::string {
    ensure_qt_application();
    return json(::inventory::inspect(text(nand)));
  }

  auto preflight(std::string_view seed_path, std::string_view reference,
                 std::string_view candidate, BootPolicy boot_policy,
                 const std::vector<SettingOverride>& settings) -> std::string {
    ensure_qt_application();
    nand_build::Options options;
    switch (boot_policy) {
    case BootPolicy::stock:
      options.boot_policy = ::inspect::BootPolicy::Stock;
      break;
    case BootPolicy::preserve_source:
      options.boot_policy = ::inspect::BootPolicy::PreserveSource;
      break;
    default:
      throw std::runtime_error("Unsupported boot policy");
    }
    for (const auto& setting : settings) {
      const auto key = text(setting.key);
      if (options.setting_overrides.contains(key))
        throw std::runtime_error("Duplicate setting override");
      options.setting_overrides.insert(key, text(setting.value));
    }
    return json(::preflight::inspect(text(seed_path), text(reference), text(candidate), options));
  }

  auto inspect(std::string_view nand, Inspection scope) -> std::string {
    ensure_qt_application();
    ::inventory::Scope native_scope;
    QString section;
    switch (scope) {
    case Inspection::Display:
      native_scope = ::inventory::Scope::Display;
      section = "display";
      break;
    case Inspection::SystemSettings:
      native_scope = ::inventory::Scope::Display;
      section = "system_settings";
      break;
    case Inspection::DisplaySource:
      native_scope = ::inventory::Scope::DisplaySource;
      section = "display_source";
      break;
    case Inspection::Priiloader:
      native_scope = ::inventory::Scope::Priiloader;
      section = "priiloader";
      break;
    default:
      throw std::runtime_error("Unsupported inspection scope");
    }
    const auto report = passed_report(text(nand), native_scope);
    return json(report.value(section).toObject());
  }

  auto display_source(std::string_view authenticated_reference) -> DisplaySource {
    ensure_qt_application();
    const auto report = passed_report(text(authenticated_reference), ::inventory::Scope::DisplaySource);
    const auto sysconf = QByteArray::fromBase64(report.value("display_source").toObject()
                                                    .value("sysconf").toString().toLatin1(),
                                                QByteArray::AbortOnBase64DecodingErrors);
    const auto source = report.value("display_source").toObject();
    const auto keys_hash = source.value("keys_data_sha256").toString();
    const auto settings_hash = source.value("settings_sha256").toString();
    if (sysconf.size() != sysconf::size || keys_hash.size() != 64 || settings_hash.size() != 64)
      throw std::runtime_error("Invalid authenticated display reference response");
    return {bytes(sysconf), keys_hash.toStdString(), settings_hash.toStdString()};
  }

  auto priiloader_app(std::string_view nand) -> std::vector<std::uint8_t> {
    ensure_qt_application();
    const auto report = passed_report(text(nand), ::inventory::Scope::PriiloaderApp);
    const auto app = QByteArray::fromBase64(report.value("priiloader_app").toString().toLatin1(),
                                            QByteArray::AbortOnBase64DecodingErrors);
    if (!priiloader::is_release_app(app))
      throw std::runtime_error("Installed Priiloader app does not match the pinned release");
    return bytes(app);
  }

  auto inspect_wad(std::string_view wad, std::string_view expected_sha256,
                   WadPolicy policy) -> std::string {
    ensure_qt_application();
    if (policy != WadPolicy::standard && policy != WadPolicy::legacy_channel)
      throw std::runtime_error("Unsupported WAD signature policy");
    if (policy == WadPolicy::legacy_channel && expected_sha256.empty())
      throw std::runtime_error("Unverified-signature WAD inspection requires an expected SHA-256");
    const auto expected = QByteArray::fromStdString(std::string(expected_sha256));
    const auto native_policy = policy == WadPolicy::legacy_channel ? wad_package::Policy::LegacyChannel
                                                                  : wad_package::Policy::Standard;
    const auto package = wad_package::read(text(wad), expected, native_policy);
    Tmd tmd(package.job.data[0]);
    const auto content_sha256 = [](const QByteArray& content) {
      return digest(content);
    };
    QJsonArray contents;
    for (int i = 0; i < tmd.Count(); ++i)
      contents.append(QJsonObject{{"cid", tmd.Cid(i)}, {"index", tmd.Index(i)}, {"type", tmd.Type(i)},
                                  {"size", package.job.data[i + 2].size()},
                                  {"sha1", QString::fromLatin1(tmd.Hash(i).toHex())},
                                  {"sha256", content_sha256(package.job.data[i + 2])}});
    const auto title_id = QString("%1").arg(package.job.tid, 16, 16, QChar('0'));
    const auto required_ios = QString("%1").arg(tmd.IOS(), 16, 16, QChar('0'));
    const QJsonObject report{
        {"sha256", QString::fromLatin1(package.sha256)},
        {"title_id", title_id},
        {"version", package.job.version},
        {"content_count", tmd.Count()},
        {"tmd_sha256", content_sha256(package.job.data[0])},
        {"required_ios", required_ios},
        {"region_code", qFromBigEndian<quint16>(package.job.data[0].constData() + 0x19c)},
        {"nintendo_signed", package.nintendo_signed()},
        {"tmd_nintendo_signed", package.tmd_nintendo_signed},
        {"ticket_nintendo_signed", package.ticket_nintendo_signed},
        {"source_common_key_index", package.source_common_key_index},
        {"source_ticket_sha256", QString::fromLatin1(package.source_ticket_sha256)},
        {"prepared_ticket_sha256", content_sha256(package.job.data[1])},
        {"contents", contents},
    };
    return json(report);
  }

  auto stock_titles(std::string_view area) -> std::vector<StockTitle> {
    ensure_qt_application();
    const auto titles = factory::stock_titles(text(area));
    std::vector<StockTitle> result;
    result.reserve(static_cast<std::size_t>(titles.size()));
    for (auto it = titles.cbegin(); it != titles.cend(); ++it)
      result.push_back({static_cast<std::uint64_t>(it.key()), static_cast<std::uint16_t>(it.value())});
    return result;
  }

} // namespace wiiform::read
