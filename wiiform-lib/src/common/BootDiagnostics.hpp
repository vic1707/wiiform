// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Inspection.hpp>

namespace inspect {
  // Accepts unwritten ECC only on already validated redundant boot2 maps.
  void verify_boot_ecc(const QByteArray& boots, BootPolicy policy, const QSet<int>& map_blocks);
}
