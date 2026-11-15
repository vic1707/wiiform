// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QByteArray>
#include <QString>
#include <wiiform/types.hpp>
#include <cstdint>

namespace sysconf {
  constexpr int size = 0x4000;
  constexpr auto path = "/shared2/sys/SYSCONF";

  auto create(const wiiform::SystemSettings& settings, const QString& menu_region) -> QByteArray;
} // namespace sysconf
