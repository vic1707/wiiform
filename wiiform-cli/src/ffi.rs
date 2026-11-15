// SPDX-License-Identifier: GPL-2.0-only
#[cxx::bridge(namespace = "wiiform::ffi")]
pub mod bridge {
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum WadPolicy {
        Standard,
        LegacyChannel,
        HomebrewChannel,
    }

    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum BootMiiState {
        Detected,
        Stock,
        Unknown,
    }

    struct SystemSettings<'a> {
        nickname: &'a str,
        language: u8,
        country: u8,
        sensor_bar_above: bool,
        widescreen: bool,
        progressive: bool,
        pal60: bool,
    }

    struct PriiloaderSettings {
        autoboot: u8,
        return_to: u8,
        shutdown_to: u8,
        preferred_mount_point: u8,
        stop_disc: bool,
        lid_slot_on_error: bool,
        ignore_shutdown_mode: bool,
        black_background: bool,
        dump_gecko_text: bool,
        show_rc_updates: bool,
    }

    struct StockOptions {
        menu: u16,
        boot_policy: u8,
        offline: bool,
    }

    struct StockIdentity {
        boot_area: [u8; 32],
        original_footer: [u8; 32],
    }

    struct ConsoleSettings {
        area: String,
        model: String,
        dvd: String,
        mpch: String,
        code: String,
        serial: String,
        video: String,
        game: String,
    }

    struct StockSeed<'a> {
        keys: &'a [u8],
        boot_blocks: &'a [u8],
        bad_blocks: &'a [u16],
        settings: &'a ConsoleSettings,
        settings_sha256: [u8; 32],
    }

    struct SeedParts {
        keys: Vec<u8>,
        boot_blocks: Vec<u8>,
        bad_blocks: Vec<u16>,
        settings: ConsoleSettings,
        settings_sha256: [u8; 32],
        bootmii: BootMiiState,
        footer_header: String,
    }

    struct CiosOptions {
        slot: u8,
        base: u8,
        revision: u16,
        replace_stock_stub: bool,
    }

    struct InstalledWad {
        title_id: u64,
        region: u16,
        version: u16,
        already_present: bool,
    }

    unsafe extern "C++" {
        include!("ffi.hpp");

        fn extract_seed(nand: &[u8], sidecar_keys: &[u8]) -> Result<SeedParts>;
        fn create_stock(
            seed: &StockSeed<'_>,
            nand: &mut [u8],
            nus_cache_dir: &str,
            options: &StockOptions,
        ) -> Result<StockIdentity>;
        fn apply_system_settings(nand: &mut [u8], settings: &SystemSettings<'_>) -> Result<()>;
        fn install_nus_title(
            nand: &mut [u8],
            nus_cache_dir: &str,
            title_id: u64,
            version: u16,
            offline: bool,
        ) -> Result<()>;
        fn install_wad(
            nand: &mut [u8],
            wad_bytes: &[u8],
            expected_sha256: &[u8; 32],
            policy: WadPolicy,
        ) -> Result<InstalledWad>;
        fn install_cios(
            nand: &mut [u8],
            wad_bytes: &[u8],
            expected_sha256: &[u8; 32],
            options: &CiosOptions,
        ) -> Result<InstalledWad>;
        fn remove_title(nand: &mut [u8], title_id: u64) -> Result<()>;
        fn apply_priiloader(
            nand: &mut [u8],
            app_bytes: &[u8],
            hacks_hash_ini: &[u8],
            settings: &PriiloaderSettings,
            enabled_hacks: &[u8],
        ) -> Result<()>;
        fn prepare_backupmii(nand: &mut [u8], expected: &StockIdentity) -> Result<Vec<u8>>;
    }
}
