// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <tools.h>

namespace title_install {
  // Install one exact Nintendo title version into an existing, footer-bearing
  // NAND image. The package is loaded and authenticated by factory::read_title
  // before the image is opened for writing.
  void install_nus_title(const QString& image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline);

  // Add a pinned channel WAD. Legacy policy is restricted to downloadable channels.
  void install_wad(const QString& image, const QString& package, const QByteArray& expected_sha256,
                   bool legacy = false);

  namespace detail {
    // Test seam for capacity and persistence tests with synthetic metadata.
    void install_validated_job(const QString& image, NusJob job);
  }
} // namespace title_install
