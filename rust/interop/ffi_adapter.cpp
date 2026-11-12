// SPDX-License-Identifier: GPL-2.0-only
#include "wiiform/rust/ffi.rs.h"
#include <wiiform/write.hpp>

namespace wiiform::ffi {
    namespace {
        auto settings_from(rust::Slice<const SettingOverride> values) -> std::vector<wiiform::SettingOverride> {
            std::vector<wiiform::SettingOverride> settings;
            settings.reserve(values.size());
            for (const auto& item : values)
                settings.push_back({std::string(item.name), std::string(item.value)});
            return settings;
        }

        auto source_from(const DisplaySource& source) -> wiiform::DisplaySource {
            return {
                {source.sysconf.begin(), source.sysconf.end()},
                std::string(source.keys_data_sha256),
                std::string(source.settings_sha256),
            };
        }
    }

    void create(rust::Str seed, rust::Str output, BootPolicy policy, rust::Slice<const SettingOverride> settings) {
        const auto boot = policy == BootPolicy::Stock ? wiiform::BootPolicy::stock : wiiform::BootPolicy::preserve_source;
        core::create(std::string(seed), std::string(output), boot, settings_from(settings));
    }

    void install_nus_title(rust::Str image, rust::Str cache, std::uint64_t title, std::uint16_t version, bool offline) {
        core::install_nus_title(std::string(image), std::string(cache), title, version, offline);
    }

    void install_wad(rust::Str image, rust::Str package, rust::Str digest, bool allow_unsigned) {
        const auto policy = allow_unsigned ? WadPolicy::legacy_channel : WadPolicy::standard;
        core::install_wad(std::string(image), std::string(package), std::string(digest), policy);
    }

    void remove_title(rust::Str image, std::uint64_t title) {
        core::remove_title(std::string(image), title);
    }

    auto set_display(rust::Str image, const DisplaySource& source, rust::Slice<const SettingOverride> settings) -> rust::String {
        return rust::String(core::set_display(std::string(image), source_from(source), settings_from(settings)));
    }

    auto set_sysconf(rust::Str image, const DisplaySource& source, rust::Slice<const SettingOverride> settings) -> rust::String {
        return rust::String(core::set_sysconf(std::string(image), source_from(source), settings_from(settings)));
    }

    auto install_priiloader(rust::Str image, rust::Str app, rust::Str output,
                           rust::Slice<const SettingOverride> settings) -> rust::String {
        return rust::String(core::install_priiloader(std::string(image), std::string(app),
                                                     std::string(output), settings_from(settings)));
    }

    auto set_priiloader_settings(rust::Str image, rust::Str output, rust::Slice<const SettingOverride> settings,
                                rust::Str definitions, rust::Slice<const SettingOverride> hacks) -> rust::String {
        return rust::String(core::set_priiloader_settings(std::string(image), std::string(output), settings_from(settings),
                                                          std::string(definitions), settings_from(hacks)));
    }

    auto normalize_backupmii_footer(rust::Str image, rust::Str expected_sha256) -> rust::String {
        return rust::String(core::normalize_backupmii_footer(std::string(image), std::string(expected_sha256)));
    }

    auto prepare_backupmii_keys(rust::Str image, rust::Str output, rust::Str expected_sha256) -> rust::String {
        return rust::String(core::prepare_backupmii_keys(std::string(image), std::string(output), std::string(expected_sha256)));
    }
}
