// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QMap>
#include <QSet>
#include <QString>
#include <nandbin.h>

namespace inspect {
  enum class BootPolicy { Stock, PreserveSource };

  struct FilesystemSummary {
    int checked_clusters = 0;
    int checked_files = 0;
    int authenticated_metadata_copies = 0;
  };

  struct StockSummary {
    int checked_titles = 0;
    int checked_contents = 0;
  };

  struct MetadataSummary {
    int checked_entries = 0;
    int checked_titles = 0;
  };

  enum class PriiloaderLayout { Unavailable, StockBoot, UnidentifiedBoot, Installed };

  struct PriiloaderEvidence {
    PriiloaderLayout layout = PriiloaderLayout::Unavailable;
    quint16 menu_version = 0;
    quint64 system_menu_ios = 0;
    QString boot_app_path;
    QString backup_app_path;
    bool backup_tmd_nintendo_signed = false;
    bool backup_app_matches_original_tmd_hash = false;
    bool backup_app_matches_original_tmd_size = false;
  };

  // These routines only inspect the supplied data. Failures use fixed messages
  // so malformed console data is never copied into diagnostics.
  // Returns the erase blocks whose boot2 maps were validated.
  auto verify_boot(const QByteArray& raw, BootPolicy policy = BootPolicy::Stock) -> QSet<int>;
  auto priiloader_evidence(NandBin& nand) -> PriiloaderEvidence;
  auto verify_filesystem(NandBin& nand, bool generated, quint16 active_metadata) -> FilesystemSummary;
  auto verify_metadata(NandBin& nand, const QString& area) -> MetadataSummary;
  auto verify_stock(NandBin& nand, const QString& area) -> StockSummary;
  void verify_stock_titles(NandBin& nand, const QMap<quint64, quint16>& titles);
} // namespace inspect
