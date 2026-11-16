// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <Inspection.hpp>

namespace inspect {
  struct BootArea {
    // Erase blocks whose boot2 maps were validated.
    QSet<int> map_blocks;
    // Every boot2 copy is validly signed and none is fakesigned.
    bool genuine;
  };

  // What any usable boot area satisfies: a recognized boot1 and consistent
  // boot2 copies, one of which boots.
  auto verify_boot(const QByteArray& raw) -> BootArea;
  // Accepts unwritten ECC only on already validated redundant boot2 maps.
  // Returns the number of pages whose ECC does not match.
  auto verify_boot_ecc(const QByteArray& boots, const QSet<int>& map_blocks) -> int;
}
