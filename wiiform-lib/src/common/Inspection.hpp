// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QMap>
#include <QSet>
#include <QString>
#include <nandbin.h>

namespace inspect {
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
  // A boot area with only genuinely signed boot2 copies.
  void verify_stock_boot(const QByteArray& raw);
  // A boot area kept as its console has it, such as BootMii installed as boot2.
  void verify_source_boot(const QByteArray& raw);
  auto priiloader_evidence(NandBin& nand) -> PriiloaderEvidence;
  void verify_filesystem(NandBin& nand, bool generated, quint16 active_metadata);
  void verify_stock(NandBin& nand, const QString& area);
  void verify_stock_titles(NandBin& nand, const QMap<quint64, quint16>& titles);
} // namespace inspect
