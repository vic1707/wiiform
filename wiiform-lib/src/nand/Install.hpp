// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QString>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>
#include <utility>

namespace title_install {
  void install_nus_title(std::span<std::uint8_t> image, const QString& cache, quint64 title_id,
                         quint16 version, bool offline);
  auto install_wad(std::span<std::uint8_t> image, std::span<const std::uint8_t> package,
                   const wiiform::Sha256& expected_sha256, wiiform::WadPolicy policy) -> wiiform::InstalledWad;
}
