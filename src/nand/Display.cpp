// SPDX-License-Identifier: GPL-2.0-only
#include <Display.hpp>
#include <WorkingImage.hpp>
#include <tools.h>

#include <QCryptographicHash>
#include <QTreeWidgetItem>
#include <memory>
#include <stdexcept>

namespace {
  void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
  }

  void check_ecc(NandBin& nand, quint16 entry) {
    const auto clusters = nand.GetFatsForFile(entry);
    require(clusters.size() == 1, "SYSCONF must occupy one NAND cluster");
    for (quint32 page = quint32(clusters.first()) * 8; page < quint32(clusters.first()) * 8 + 8; ++page)
      require(nand.CheckEcc(page), "SYSCONF page ECC is invalid");
  }

  auto sha256(const QByteArray& data) -> QByteArray {
    return QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex();
  }

  void check_source(NandBin& nand, QTreeWidgetItem* tree, const sysconf::Source& source) {
    const auto keys = nand.Keys();
    require(keys.size() == 0x400 && source.keys_data_sha256.size() == 64 &&
                source.keys_data_sha256 == sha256(keys.mid(sysconf::key_header_bytes)),
            "Display reference belongs to a different console");
    const auto* settings = working_image::find_path(tree, "/title/00000001/00000002/data/setting.txt");
    const auto data = working_image::read_authenticated_file(nand, settings, 256);
    require(data.size() == 256 && sha256(data) == source.settings_sha256,
            "Display reference and target have different setting.txt bytes");
  }

  void persist(working_image::Image& working, const QString& image, QTreeWidgetItem* tree,
               quint16 id, bool initialize, const QByteArray& edited) {
    auto& nand = working.nand;
    if (initialize) {
      const auto* parent = working_image::find_path(tree, "/shared2/sys");
      require(parent && working_image::entry_kind(parent) == NAND_DIR, "Working image needs /shared2/sys");
      working_image::require_capacity(nand, tree->child(0), working.layout.orphan_entries, 1, 1);
      // System Menu ownership and permissions observed in original Wii captures.
      id = nand.CreateEntry(sysconf::path, 0x1000, 1, NAND_FILE, NAND_RW, NAND_RW, NAND_RW);
      require(id != 0, "Cannot create SYSCONF in the working image");
    }
    require(nand.SetData(id, edited) && nand.WriteMetaData(), "Cannot persist system settings");
    const auto layout = inspect::validate_layout(image, false);
    NandBin reopened;
    require(reopened.SetPath(image) && reopened.InitNand() && reopened.CheckHmacMeta(layout.active_metadata),
            "Cannot authenticate the edited working image");
    const std::unique_ptr<QTreeWidgetItem> verified(reopened.GetTree());
    const auto* verified_entry = working_image::find_path(verified.get(), sysconf::path);
    require(working_image::read_authenticated_file(reopened, verified_entry, sysconf::size) == edited,
            "SYSCONF readback differs from the requested edit");
    check_ecc(reopened, working_image::entry_id(verified_entry));
  }

  template<typename Patch, typename Describe>
  auto edit_sysconf(const QString& image, const std::optional<sysconf::Source>& source,
                    Patch patch, Describe describe) -> QJsonObject {
    working_image::Image working(image);
    auto& nand = working.nand;
    const std::unique_ptr<QTreeWidgetItem> tree(nand.GetTree());
    const auto* entry = working_image::find_path(tree.get(), sysconf::path);
    quint16 id = 0;
    QByteArray original;
    if (source) {
      require(!entry, "SYSCONF import needs a working image without SYSCONF");
      check_source(nand, tree.get(), *source);
      original = source->bytes;
    } else {
      require(entry, "Working image needs an existing SYSCONF or --from REFERENCE_NAND");
      original = working_image::read_authenticated_file(nand, entry, sysconf::size);
      id = working_image::entry_id(entry);
      check_ecc(nand, id);
    }
    const auto edited = patch(original);
    auto report = describe(edited);
    report["changed"] = source.has_value() || edited != original;
    report["sysconf_sha256"] = QString::fromLatin1(sha256(edited));
    if (!report["changed"].toBool()) return report;

    if (source) {
      report["initialized"] = true;
      report["source_sysconf_sha256"] = QString::fromLatin1(sha256(original));
    }
    persist(working, image, tree.get(), id, source.has_value(), edited);
    return report;
  }
}

namespace sysconf {
  auto set_display(const QString& image, const DisplayOverrides& overrides,
                   const std::optional<Source>& source) -> QJsonObject {
    return edit_sysconf(image, source,
                       [&](const auto& bytes) { return patch_display(bytes, overrides); },
                       [](const auto& bytes) { return display_report(read_display(bytes)); });
  }

  auto set_settings(const QString& image, const SettingsOverrides& overrides,
                    const std::optional<Source>& source) -> QJsonObject {
    require(overrides.require_setup || overrides.nickname || overrides.language ||
                overrides.display.widescreen || overrides.display.progressive_scan ||
                overrides.display.pal60 || source,
            "No system settings were requested");
    return edit_sysconf(image, source, [&](const auto& bytes) { return patch_settings(bytes, overrides); },
                       [](const auto& bytes) {
      auto report = display_report(read_display(bytes));
      const auto settings = settings_report(read_settings(bytes));
      for (auto it = settings.begin(); it != settings.end(); ++it) report[it.key()] = it.value();
      return report;
    });
  }
}
