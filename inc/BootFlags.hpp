// SPDX-License-Identifier: GPL-2.0-only
#pragma once

namespace boot {
  // WiiQt represents boot2 status as a bit field. BAD_CONTENT_HASH can be
  // combined with signature and boot-selection flags.
  constexpr int bad_content_hash_flag = 1;

  constexpr auto has_bad_content_hash(int state) -> bool {
    return state >= 0 && (state & bad_content_hash_flag) != 0;
  }
} // namespace boot
