// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Inspection.hpp>
#include <QByteArray>
#include <QJsonArray>

namespace preflight {
  struct BootEccDiagnostics {
    QJsonArray pages;
    bool blocked = false;
  };

  // Classifies only ECC mismatches that are explained by valid, redundant
  // boot2 maps in a boot area validated under the selected policy and identical to its seed.
  auto diagnose_boot_ecc(const QByteArray& boots, const QByteArray& seed_boots,
                         inspect::BootPolicy policy = inspect::BootPolicy::Stock) -> BootEccDiagnostics;
} // namespace preflight
