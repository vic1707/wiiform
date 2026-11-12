// SPDX-License-Identifier: GPL-2.0-only
#[cxx::bridge(namespace = "wiiform::ffi")]
pub mod bridge {
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum BootPolicy {
        Stock,
        PreserveSource,
    }

    #[derive(Clone, Debug)]
    struct SettingOverride {
        name: String,
        value: String,
    }

    #[derive(Default)]
    struct DisplaySource {
        sysconf: Vec<u8>,
        keys_data_sha256: String,
        settings_sha256: String,
    }

    unsafe extern "C++" {
        include!("ffi.hpp");

        fn create(
            seed: &str,
            output: &str,
            policy: BootPolicy,
            settings: &[SettingOverride],
        ) -> Result<()>;
        fn install_nus_title(
            image: &str,
            cache: &str,
            title: u64,
            version: u16,
            offline: bool,
        ) -> Result<()>;
        fn install_wad(
            image: &str,
            package: &str,
            digest: &str,
            allow_unsigned: bool,
        ) -> Result<()>;
        fn remove_title(image: &str, title: u64) -> Result<()>;
        fn set_display(
            image: &str,
            source: &DisplaySource,
            settings: &[SettingOverride],
        ) -> Result<String>;
        fn set_sysconf(
            image: &str,
            source: &DisplaySource,
            settings: &[SettingOverride],
        ) -> Result<String>;
        fn install_priiloader(
            image: &str,
            app: &str,
            output: &str,
            settings: &[SettingOverride],
        ) -> Result<String>;
        fn set_priiloader_settings(
            image: &str,
            output: &str,
            settings: &[SettingOverride],
            definitions: &str,
            hacks: &[SettingOverride],
        ) -> Result<String>;
        fn normalize_backupmii_footer(image: &str, expected_sha256: &str) -> Result<String>;
        fn prepare_backupmii_keys(
            image: &str,
            output: &str,
            expected_sha256: &str,
        ) -> Result<String>;
    }
}
