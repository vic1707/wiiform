// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include "rust/cxx.h"
#include <cstdint>
#include <vector>

namespace wiiform::ffi {
    enum class BootPolicy : std::uint8_t;
    enum class Inspection : std::uint8_t;
    enum class WadPolicy : std::uint8_t;
    struct SettingOverride;
    struct DisplaySource;
    struct StockTitle;

    void export_seed(rust::Str nand, rust::Str seed);
    void unpack_seed(rust::Str seed, rust::Str directory);
    auto inventory(rust::Str nand) -> rust::String;
    auto preflight(rust::Str seed, rust::Str reference, rust::Str candidate,
                   BootPolicy policy, rust::Slice<const SettingOverride> settings) -> rust::String;
    auto inspect(rust::Str nand, Inspection scope) -> rust::String;
    auto display_source(rust::Str nand) -> DisplaySource;
    auto priiloader_app(rust::Str nand) -> rust::Vec<std::uint8_t>;
    auto inspect_wad(rust::Str wad, rust::Str digest, WadPolicy policy) -> rust::String;
    auto stock_titles(rust::Str area) -> rust::Vec<StockTitle>;

    void create(rust::Str seed, rust::Str output, BootPolicy policy, rust::Slice<const SettingOverride> settings);
    void install_nus_title(rust::Str image, rust::Str cache, std::uint64_t title, std::uint16_t version, bool offline);
    void install_wad(rust::Str image, rust::Str package, rust::Str digest, bool allow_unsigned);
    void remove_title(rust::Str image, std::uint64_t title);
    auto set_display(rust::Str image, const DisplaySource& source, rust::Slice<const SettingOverride> settings) -> rust::String;
    auto set_sysconf(rust::Str image, const DisplaySource& source, rust::Slice<const SettingOverride> settings) -> rust::String;
    auto install_priiloader(rust::Str image, rust::Str app, rust::Str output,
                           rust::Slice<const SettingOverride> settings) -> rust::String;
    auto set_priiloader_settings(rust::Str image, rust::Str output, rust::Slice<const SettingOverride> settings,
                                rust::Str definitions, rust::Slice<const SettingOverride> hacks) -> rust::String;
    auto normalize_backupmii_footer(rust::Str image, rust::Str expected_sha256) -> rust::String;
    auto prepare_backupmii_keys(rust::Str image, rust::Str output, rust::Str expected_sha256) -> rust::String;
}
