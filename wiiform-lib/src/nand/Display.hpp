// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <SysConf.hpp>
#include <cstdint>
#include <span>

namespace sysconf {
  void initialize(std::span<std::uint8_t> image, const SettingsOverrides& overrides);
}
