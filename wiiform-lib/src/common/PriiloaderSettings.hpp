// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <wiiform/types.hpp>

namespace priiloader {
  // Encodes the pinned Priiloader 0.10.0 loader.ini layout.
  auto encode_settings(const wiiform::PriiloaderSettings& settings) -> QByteArray;
}
