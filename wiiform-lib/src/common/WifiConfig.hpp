// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <QtGlobal>

namespace wifi_config {
  // The Wi-Fi profile file the console reads: one 0x4000-byte NAND cluster.
  constexpr int size = 7004;
  constexpr auto path = "/shared2/sys/net/02/config.dat";
  // Owner of the network directories and this file in original captures.
  constexpr quint32 owner = 13;
  constexpr quint16 group = 13;
} // namespace wifi_config
