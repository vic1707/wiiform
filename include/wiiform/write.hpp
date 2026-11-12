// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <wiiform/types.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace wiiform::core {
  void create(const std::string& seed, const std::string& output,
              BootPolicy boot_policy,
              const std::vector<SettingOverride>& setting_overrides);

  void install_nus_title(const std::string& image, const std::string& cache,
                         std::uint64_t title_id, std::uint16_t version, bool offline);

  void install_wad(const std::string& image, const std::string& package,
                   const std::string& expected_sha256, WadPolicy policy);
  void remove_title(const std::string& image, std::uint64_t title_id);

  // Keys use the existing wii-nand option names: aspect, progressive, pal60,
  // nickname, language, and require-setup (the latter accepts "true").
  [[nodiscard]] std::string set_display(
      const std::string& image, const DisplaySource& source,
      const std::vector<SettingOverride>& overrides);
  [[nodiscard]] std::string set_sysconf(
      const std::string& image, const DisplaySource& source,
      const std::vector<SettingOverride>& overrides);

  // Priiloader's values are the exact `--setting-override` names/values. Hack
  // overrides map the full hack title to "on" or "off".
  [[nodiscard]] std::string install_priiloader(
      const std::string& input_image, const std::string& app,
      const std::string& output_image,
      const std::vector<SettingOverride>& settings);
  [[nodiscard]] std::string set_priiloader_settings(
      const std::string& input_image, const std::string& output_image,
      const std::vector<SettingOverride>& settings,
      const std::string& hack_definitions,
      const std::vector<SettingOverride>& hacks);

  // These operate only on a generated footer-bearing image. The expected hash
  // authenticates the original 1024-byte footer before any read or write.
  // normalize returns an empty string when the footer already has a BackupMii
  // header, otherwise the normalized footer's lowercase SHA-256.
  [[nodiscard]] std::string normalize_backupmii_footer(
      const std::string& image, const std::string& expected_sha256);
  [[nodiscard]] std::string prepare_backupmii_keys(
      const std::string& image, const std::string& target,
      const std::string& expected_sha256);
}
