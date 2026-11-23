// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <cstdint>
#include <span>

namespace wifi_config {
  // Creates the Wi-Fi profile file with any missing directories above it. `config` is
  // the whole file, 7004 bytes. Fails if the image already has the file.
  void initialize(std::span<std::uint8_t> image, std::span<const std::uint8_t> config);
}
