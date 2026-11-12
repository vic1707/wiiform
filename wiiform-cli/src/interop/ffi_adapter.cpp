// SPDX-License-Identifier: GPL-2.0-only
#include "wiiform/src/ffi.rs.h"
#include <wiiform/read.hpp>
#include <wiiform/write.hpp>
#include <stdexcept>

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

        auto source_to(const wiiform::DisplaySource& source) -> DisplaySource {
            DisplaySource result;
            result.sysconf.reserve(source.sysconf.size());
            for (const auto byte : source.sysconf) result.sysconf.push_back(byte);
            result.keys_data_sha256 = source.keys_data_sha256;
            result.settings_sha256 = source.settings_sha256;
            return result;
        }

        auto boot_policy_from(BootPolicy policy) -> wiiform::BootPolicy {
            switch (policy) {
            case BootPolicy::Stock: return wiiform::BootPolicy::stock;
            case BootPolicy::PreserveSource: return wiiform::BootPolicy::preserve_source;
            default: throw std::runtime_error("Unsupported boot policy");
            }
        }

        auto inspection_from(Inspection scope) -> wiiform::read::Inspection {
            switch (scope) {
            case Inspection::Display: return wiiform::read::Inspection::Display;
            case Inspection::SystemSettings: return wiiform::read::Inspection::SystemSettings;
            case Inspection::DisplaySource: return wiiform::read::Inspection::DisplaySource;
            case Inspection::Priiloader: return wiiform::read::Inspection::Priiloader;
            default: throw std::runtime_error("Unsupported inspection scope");
            }
        }

        auto wad_policy_from(WadPolicy policy) -> wiiform::WadPolicy {
            switch (policy) {
            case WadPolicy::Standard: return wiiform::WadPolicy::standard;
            case WadPolicy::LegacyChannel: return wiiform::WadPolicy::legacy_channel;
            default: throw std::runtime_error("Unsupported WAD signature policy");
            }
        }
    }

    void export_seed(rust::Str nand, rust::Str seed) {
        wiiform::read::export_seed(std::string(nand), std::string(seed));
    }

    void unpack_seed(rust::Str seed, rust::Str directory) {
        wiiform::read::unpack_seed(std::string(seed), std::string(directory));
    }

    auto inventory(rust::Str nand) -> rust::String {
        return rust::String(wiiform::read::inventory(std::string(nand)));
    }

    auto preflight(rust::Str seed, rust::Str reference, rust::Str candidate,
                   BootPolicy policy, rust::Slice<const SettingOverride> settings) -> rust::String {
        return rust::String(wiiform::read::preflight(std::string(seed), std::string(reference),
                                                     std::string(candidate), boot_policy_from(policy),
                                                     settings_from(settings)));
    }

    auto inspect(rust::Str nand, Inspection scope) -> rust::String {
        return rust::String(wiiform::read::inspect(std::string(nand), inspection_from(scope)));
    }

    auto display_source(rust::Str nand) -> DisplaySource {
        return source_to(wiiform::read::display_source(std::string(nand)));
    }

    auto priiloader_app(rust::Str nand) -> rust::Vec<std::uint8_t> {
        rust::Vec<std::uint8_t> result;
        const auto app = wiiform::read::priiloader_app(std::string(nand));
        result.reserve(app.size());
        for (const auto byte : app) result.push_back(byte);
        return result;
    }

    auto inspect_wad(rust::Str wad, rust::Str digest, WadPolicy policy) -> rust::String {
        return rust::String(wiiform::read::inspect_wad(std::string(wad), std::string(digest),
                                                       wad_policy_from(policy)));
    }

    auto stock_titles(rust::Str area) -> rust::Vec<StockTitle> {
        rust::Vec<StockTitle> result;
        for (const auto& title : wiiform::read::stock_titles(std::string(area)))
            result.push_back(StockTitle{title.title_id, title.version});
        return result;
    }

    void create(rust::Str seed, rust::Str output, BootPolicy policy, rust::Slice<const SettingOverride> settings) {
        core::create(std::string(seed), std::string(output), boot_policy_from(policy), settings_from(settings));
    }

    void install_nus_title(rust::Str image, rust::Str cache, std::uint64_t title, std::uint16_t version, bool offline) {
        core::install_nus_title(std::string(image), std::string(cache), title, version, offline);
    }

    void install_wad(rust::Str image, rust::Str package, rust::Str digest, bool allow_unsigned) {
        const auto policy = allow_unsigned ? wiiform::WadPolicy::legacy_channel : wiiform::WadPolicy::standard;
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
