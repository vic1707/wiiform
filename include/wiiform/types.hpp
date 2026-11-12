// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <string>
#include <cstdint>
#include <vector>

namespace wiiform {
  enum class BootPolicy { stock, preserve_source };
  enum class WadPolicy { standard, legacy_channel };

  struct SettingOverride {
    std::string key;
    std::string value;
  };

  struct DisplaySource {
    std::vector<std::uint8_t> sysconf;
    std::string keys_data_sha256;
    std::string settings_sha256;
  };
}
