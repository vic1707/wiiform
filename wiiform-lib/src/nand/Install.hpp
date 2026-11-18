// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <TitleValidation.hpp>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>
#include <utility>

namespace title_install {
  auto install_nus_title(std::span<std::uint8_t> image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline) -> wiiform::InstalledWad;
  // Installs a package one of the wad_package readers accepted.
  auto install_wad(std::span<std::uint8_t> image, const ValidatedTitleJob& job) -> wiiform::InstalledWad;
  auto install_cios(std::span<std::uint8_t> image, std::span<const std::uint8_t> package,
                    const wiiform::CiosOptions& options) -> wiiform::InstalledWad;
  auto install_bootmii_ios(std::span<std::uint8_t> image, const QByteArray& tmd, const QByteArray& ticket,
                          const QByteArray& content) -> wiiform::InstalledWad;
  auto install_built_cios(std::span<std::uint8_t> image, const QString& cache, bool offline,
                          const wiiform::CiosBuild& build) -> wiiform::InstalledWad;
  auto install_rvloader_ios(std::span<std::uint8_t> image, const QString& cache, bool offline,
                            const wiiform::RvloaderIos& ios) -> wiiform::InstalledWad;
}
