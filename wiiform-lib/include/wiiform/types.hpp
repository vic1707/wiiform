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
  enum class BootMiiState : std::uint8_t { detected, stock, unknown };

  inline constexpr std::size_t raw_nand_size = 0x21000000;
  inline constexpr std::size_t nand_size = raw_nand_size + 0x400;
  using Sha256 = std::array<std::uint8_t, 32>;

  enum class Menu : std::uint16_t { e43 = 514, u43 = 513, j43 = 512 };
  struct ConsoleSettings {
    std::string area;
    std::string model;
    std::string dvd;
    std::string mpch;
    std::string code;
    std::string serial;
    std::string video;
    std::string game;
  };
  struct SeedParts {
    std::vector<std::uint8_t> keys;
    std::vector<std::uint8_t> boot_blocks;
    std::vector<std::uint16_t> bad_blocks;
    ConsoleSettings settings;
    Sha256 settings_sha256;
    BootMiiState bootmii;
    std::string footer_header;
  };
  struct StockSeed {
    std::span<const std::uint8_t> keys;
    std::span<const std::uint8_t> boot_blocks;
    std::span<const std::uint16_t> bad_blocks;
    const ConsoleSettings& settings;
    Sha256 settings_sha256;
  };

  struct StockIdentity {
    Sha256 boot_area;
    Sha256 original_footer;
  };

  struct InstalledWad {
    std::uint64_t title_id;
    std::uint16_t region;
    std::uint16_t version;
    bool already_present;
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
    Autoboot autoboot = Autoboot::system_menu;
    ReturnTo return_to = ReturnTo::system_menu;
    ShutdownTo shutdown_to = ShutdownTo::none;
    MountPoint preferred_mount_point = MountPoint::automatic;
    bool stop_disc = false;
    bool lid_slot_on_error = false;
    bool ignore_shutdown_mode = false;
    bool black_background = true;
    bool dump_gecko_text = false;
    bool show_rc_updates = false;
  };

  enum class Hack : std::uint8_t {
    fix_480p = 0, block_online_updates = 1, block_disc_updates = 2, block_disc_autoboot = 3,
    auto_press_a = 4, remove_deflicker = 5, move_disc_channel = 6, remove_nocopy_save_protection = 7,
    replace_health_screen = 8, no_background_music = 9, wiimmfi = 10, region_free_everything = 11
  };

}
