// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <array>
#include <optional>
#include <span>
#include <string>
#include <cstdint>
#include <vector>

namespace wiiform {
  enum class BootPolicy { stock, preserve_source };
  enum class WadPolicy { standard, legacy_channel };

  inline constexpr std::size_t raw_nand_size = 0x21000000;
  inline constexpr std::size_t nand_size = raw_nand_size + 0x400;
  using Sha256 = std::array<std::uint8_t, 32>;

  enum class Menu { e43, u43, j43 };
  enum class CacheMode { offline, download_missing };

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

  enum class Language { ja, en, de, fr, es, it, nl };

  struct SystemSettings {
    std::string nickname;
    std::optional<Language> language;
    std::optional<std::uint8_t> country;
    std::optional<bool> sensor_bar_above;
    std::optional<bool> widescreen;
    std::optional<bool> progressive;
    std::optional<bool> pal60;
  };

  enum class Autoboot { disabled, homebrew_channel, bootmii_ios, system_menu, file };
  enum class ReturnTo { system_menu, priiloader, autoboot, file };
  enum class ShutdownTo { none, priiloader, autoboot };
  enum class MountPoint { automatic, sd, usb };

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

  enum class Hack {
    fix_480p, block_online_updates, block_disc_updates, block_disc_autoboot, auto_press_a,
    remove_deflicker, move_disc_channel, remove_nocopy_save_protection, replace_health_screen,
    no_background_music, wiimmfi, region_free_everything
  };

  struct HackOverride {
    Hack hack;
    bool enabled;
  };

}
