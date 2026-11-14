// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QMap>
#include <QString>
#include <wiiform/types.hpp>
#include <cstdint>
#include <span>

namespace priiloader {
  void apply(std::span<std::uint8_t> image, std::span<const std::uint8_t> app,
             std::span<const std::uint8_t> hack_definitions,
             const wiiform::PriiloaderSettings& settings,
             std::span<const wiiform::Hack> enabled_hacks);
}
