// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <array>
#include <optional>
#include <span>
#include <string>
#include <cstdint>
#include <vector>

namespace wiiform {
  enum class BootPolicy : std::uint8_t { stock = 0, preserve_source = 1 };
  enum class WadPolicy { standard, legacy_channel };

  inline constexpr std::size_t raw_nand_size = 0x21000000;
  inline constexpr std::size_t nand_size = raw_nand_size + 0x400;
  using Sha256 = std::array<std::uint8_t, 32>;

  enum class Menu : std::uint16_t { e43 = 514, u43 = 513, j43 = 512 };
  struct StockSeed {
    std::span<const std::uint8_t> keys;
    std::span<const std::uint8_t> boot_blocks;
    std::span<const std::uint16_t> bad_blocks;
    std::span<const std::uint8_t> setting_txt;
  };

  struct StockIdentity {
    Sha256 boot_area;
    Sha256 original_footer;
  };

  struct InstalledWad {
    std::uint64_t title_id;
    std::uint16_t region;
  };

  enum class Language : std::uint8_t { ja = 0, en = 1, de = 2, fr = 3, es = 4, it = 5, nl = 6 };

  struct SystemSettings {
    std::string nickname;
    Language language;
    std::uint8_t country;
    bool sensor_bar_above;
    bool widescreen;
    bool progressive;
    bool pal60;
  };

  enum class Autoboot : std::uint8_t { disabled = 0, homebrew_channel = 1, bootmii_ios = 2, system_menu = 3, file = 4 };
  enum class ReturnTo : std::uint8_t { system_menu = 0, priiloader = 1, autoboot = 2, file = 3 };
  enum class ShutdownTo : std::uint8_t { none = 0, priiloader = 1, autoboot = 2 };
  enum class MountPoint : std::uint8_t { automatic = 0, sd = 1, usb = 2 };

  struct PriiloaderSettings {
    std::optional<Autoboot> autoboot;
    std::optional<ReturnTo> return_to;
    std::optional<ShutdownTo> shutdown_to;
    std::optional<MountPoint> preferred_mount_point;
    std::optional<bool> stop_disc;
    std::optional<bool> lid_slot_on_error;
    std::optional<bool> ignore_shutdown_mode;
    std::optional<bool> black_background;
    std::optional<bool> dump_gecko_text;
    std::optional<bool> show_rc_updates;
  };

  enum class Hack : std::uint8_t {
    fix_480p = 0, block_online_updates = 1, block_disc_updates = 2, block_disc_autoboot = 3,
    auto_press_a = 4, remove_deflicker = 5, move_disc_channel = 6, remove_nocopy_save_protection = 7,
    replace_health_screen = 8, no_background_music = 9, wiimmfi = 10, region_free_everything = 11
  };

  struct HackOverride {
    Hack hack;
    bool enabled;
  };

}
