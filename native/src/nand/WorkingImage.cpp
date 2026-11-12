// SPDX-License-Identifier: GPL-2.0-only
#include <WorkingImage.hpp>
#include <tools.h>

#include <QFileInfo>
#include <QTreeWidgetItem>
#include <algorithm>
#include <cerrno>
#include <stdexcept>
#include <sys/file.h>

namespace {
  auto count_fst_entries(const QTreeWidgetItem* item) -> int {
    int count = 1;
    for (int i = 0; i < item->childCount(); ++i) count += count_fst_entries(item->child(i));
    return count;
  }
}

namespace working_image {
  Image::Image(const QString& path) {
    constexpr qint64 nand_size_with_footer = 0x21000400;
    const QFileInfo info(path);
    if (!info.isFile() || info.isSymLink() || info.size() != nand_size_with_footer)
      throw std::runtime_error("In-place edits require a regular footer-bearing NAND image");
    const auto absolute_path = info.absoluteFilePath();
    lock.setFileName(absolute_path);
    if (!lock.open(QIODevice::ReadWrite | QIODevice::ExistingOnly))
      throw std::runtime_error("NAND image is not writable");
    // The inode lock covers hard links and stays open until WiiQt closes.
    if (flock(lock.handle(), LOCK_EX | LOCK_NB) != 0) {
      if (errno == EWOULDBLOCK) throw std::runtime_error("Another edit already holds the NAND image lock");
      throw std::runtime_error("Cannot lock NAND image for editing");
    }
    layout = inspect::validate_layout(absolute_path, false);
    if (!nand.SetPath(absolute_path) || !nand.InitNand())
      throw std::runtime_error("Cannot open NAND filesystem for editing");
    if (!nand.CheckHmacMeta(layout.active_metadata))
      throw std::runtime_error("Active NAND metadata HMAC is invalid");
  }

  auto find_path(QTreeWidgetItem* tree, const QString& path) -> QTreeWidgetItem* {
    if (!tree || tree->childCount() != 1 || !path.startsWith('/')) return nullptr;
    auto* item = tree->child(0);
    if (item->text(0) != "/") return nullptr;
    for (const auto& name : path.split('/', Qt::SkipEmptyParts)) {
      QTreeWidgetItem* next = nullptr;
      for (int i = 0; i < item->childCount(); ++i) {
        if (item->child(i)->text(0) == name) {
          next = item->child(i);
          break;
        }
      }
      if (!next) return nullptr;
      item = next;
    }
    return item;
  }

  auto entry_kind(const QTreeWidgetItem* item) -> int {
    bool ok = false;
    const auto kind = item->text(7).left(2).toInt(&ok, 16);
    if (!ok) throw std::runtime_error("Invalid NAND FST entry type");
    return kind;
  }

  auto entry_id(const QTreeWidgetItem* item) -> quint16 {
    bool ok = false;
    const auto index = item->text(1).toUInt(&ok, 10);
    if (!ok || index >= 0x17ff) throw std::runtime_error("Invalid NAND FST entry index");
    return static_cast<quint16>(index);
  }

  auto read_authenticated_file(NandBin& nand, const QTreeWidgetItem* item, quint64 maximum) -> QByteArray {
    if (!item || entry_kind(item) != NAND_FILE)
      throw std::runtime_error("Expected NAND file is missing or has the wrong type");
    bool size_ok = false;
    const auto size = item->text(2).toULongLong(&size_ok, 16);
    if (!size_ok || size > maximum) throw std::runtime_error("NAND file exceeds the read limit");
    const auto index = entry_id(item);
    if (!nand.CheckHmacData(index)) throw std::runtime_error("NAND file HMAC is invalid");
    const auto data = nand.GetFile(index);
    if (quint64(data.size()) != size) throw std::runtime_error("Incomplete NAND file read");
    return data;
  }

  void require_capacity(NandBin& nand, const QTreeWidgetItem* root, int orphan_entries,
                        int needed_entries, quint64 needed_clusters) {
    constexpr int fst_slot_count = 0x17ff;
    const auto used_entries = count_fst_entries(root) + orphan_entries;
    if (used_entries > fst_slot_count || needed_entries > fst_slot_count - used_entries)
      throw std::runtime_error("Insufficient free NAND FST entries");
    const auto fats = nand.GetFats();
    if (fats.size() != 0x8000) throw std::runtime_error("Invalid NAND allocation table");
    const auto free_clusters = static_cast<quint64>(std::count(fats.cbegin(), fats.cend(), 0xfffe));
    if (needed_clusters >= free_clusters)
      throw std::runtime_error("Insufficient NAND space (one cluster is reserved)");
  }
}
