// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QJsonObject>
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

  // These routines only inspect the supplied data. Failures use fixed messages
  // so malformed console data is never copied into diagnostics.
  // Returns the erase blocks whose boot2 maps were validated.
  auto verify_boot(const QByteArray& raw, BootPolicy policy = BootPolicy::Stock) -> QSet<int>;
  auto read_file(NandBin& nand, const QString& path, qint64 limit) -> QByteArray;
  // Caller must first validate the active layout, authenticate metadata, and
  // verify live-file ECC/HMAC. This emits structure and hashes, never payloads.
  auto inventory_filesystem(NandBin& nand) -> QJsonObject;
  auto priiloader_evidence(NandBin& nand) -> QJsonObject;
  auto verify_filesystem(NandBin& nand, bool generated, quint16 active_metadata) -> FilesystemSummary;
  auto verify_metadata(NandBin& nand, const QString& area) -> MetadataSummary;
  auto verify_stock(NandBin& nand, const QString& area) -> StockSummary;
  void verify_stock_titles(NandBin& nand, const QMap<quint64, quint16>& titles);
} // namespace inspect
