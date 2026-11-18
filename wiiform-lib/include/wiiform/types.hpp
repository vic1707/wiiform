// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <array>
#include <span>
#include <string>
#include <cstdint>
#include <vector>

namespace wiiform {
  enum class BootMiiState : std::uint8_t { detected, stock, unknown };

  inline constexpr std::size_t raw_nand_size = 0x21000000;
  inline constexpr std::size_t footer_size = 0x400;
  inline constexpr std::size_t nand_size = raw_nand_size + footer_size;
  inline constexpr std::size_t boot_area_size = 0x108000;
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

  // Digests of the two regions that later edits must never change.
  struct StockIdentity {
    Sha256 boot_area;
    Sha256 footer;
  };

  struct TitleVersion {
    std::uint64_t id;
    std::uint16_t version;
  };

  // A directory holding Nintendo title files from earlier downloads. Offline, nothing is
  // downloaded and a title missing from the directory is an error.
  struct NusCache {
    std::string directory;
    bool offline;
  };

  struct CiosOptions {
    std::uint8_t slot;
    std::uint8_t base;
    std::uint16_t revision;
    bool replace_stock_stub;
  };

  // One change a d2x map makes to a content of the base IOS.
  struct ContentPatch {
    std::uint32_t cid;
    std::uint32_t offset;
    std::span<const std::uint8_t> original;
    std::span<const std::uint8_t> replacement;
  };

  // A d2x module added to the base IOS: last (position -1), or at a position
  // whose content then moves last.
  struct AddedModule {
    std::uint32_t cid;
    std::int32_t position;
    std::span<const std::uint8_t> bytes;
  };

  // A d2x cIOS to build from base IOS<options.base> v<base_version>, fetched like
  // install_nus_title.
  struct CiosBuild {
    std::uint16_t base_version;
    CiosOptions options;
    std::span<const ContentPatch> patches;
    std::span<const AddedModule> modules;
  };

  // One IOS as RVLoader's installer (v2.1) derives it from a stock one.
  struct RvloaderIos {
    // IOS36 v3608, IOS58 v6176 or IOS80 v6944, fetched like install_nus_title does.
    TitleVersion source;
    // Where the result goes. Its own slot replaces the stock version; any other must be free.
    std::uint8_t slot;
    // Patch the fakesigning bug, ES_Identify, ISFS permissions and the downgrade check out of the kernel.
    bool es_patches;
    // A Wi-Fi driver module replacing the stock WD and WL modules; empty keeps them.
    std::span<const std::uint8_t> wifi_module;
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
    replace_health_screen = 8, no_background_music = 9, wiimmfi = 10, region_free_everything = 11,
    no_sfx_and_background_music = 12, re_enable_bannerbomb = 13, usb_gecko_report = 14, usb_gecko_report_geckoos = 15, force_data_management = 16, force_recovery_mode = 17, remove_diagnostic_disc_check = 18, no_delete_homebrew = 19, force_disc_ios248 = 20, force_disc_ios249 = 21, force_disc_ios250 = 22, force_disc_ios251 = 23, unblock_service_discs = 24, remove_ios16_disc_error = 25, mark_network_tested = 26
  };

}
