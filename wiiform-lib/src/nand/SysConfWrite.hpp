// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <SysConf.hpp>
#include <cstdint>
#include <span>

namespace wii_sysconf {
  // `registrations` is the BT.DINF value, or empty for none.
  void initialize(std::span<std::uint8_t> image, const wiiform::SystemSettings& settings,
                  std::span<const std::uint8_t> registrations = {});
}
