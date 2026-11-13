// SPDX-License-Identifier: GPL-2.0-only
#[cxx::bridge(namespace = "wiiform::ffi")]
pub mod bridge {
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum BootPolicy {
        Stock,
        PreserveSource,
    }

    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum Menu {
        E43,
        U43,
        J43,
    }

    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum CacheMode {
        Offline,
        DownloadMissing,
    }

    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum WadPolicy {
        Standard,
        LegacyChannel,
    }

    #[repr(u8)]
    enum Toggle {
        Unset,
        Off,
        On,
    }

    #[repr(u8)]
    enum Language {
        Ja,
        En,
        De,
        Fr,
        Es,
        It,
        Nl,
        Unset,
    }

    struct SystemSettings {
        nickname: String,
        language: Language,
        country: u8,
        sensor_bar_above: Toggle,
        widescreen: Toggle,
        progressive: Toggle,
        pal60: Toggle,
    }

    #[repr(u8)]
    enum Autoboot {
        Disabled,
        HomebrewChannel,
        BootmiiIos,
        SystemMenu,
        File,
        Unset,
    }

    #[repr(u8)]
    enum ReturnTo {
        SystemMenu,
        Priiloader,
        Autoboot,
        File,
        Unset,
    }

    #[repr(u8)]
    enum ShutdownTo {
        None,
        Priiloader,
        Autoboot,
        Unset,
    }

    #[repr(u8)]
    enum MountPoint {
        Auto,
        Sd,
        Usb,
        Unset,
    }

    struct PriiloaderSettings {
        autoboot: Autoboot,
        return_to: ReturnTo,
        shutdown_to: ShutdownTo,
        preferred_mount_point: MountPoint,
        stop_disc: Toggle,
        lid_slot_on_error: Toggle,
        ignore_shutdown_mode: Toggle,
        black_background: Toggle,
        dump_gecko_text: Toggle,
        show_rc_updates: Toggle,
    }

    #[repr(u8)]
    enum Hack {
        Fix480p,
        BlockOnlineUpdates,
        BlockDiscUpdates,
        BlockDiscAutoboot,
        AutoPressA,
        RemoveDeflicker,
        MoveDiscChannel,
        RemoveNocopySaveProtection,
        ReplaceHealthScreen,
        NoBackgroundMusic,
        Wiimmfi,
        RegionFreeEverything,
    }

    struct HackOverride {
        hack: Hack,
        enabled: bool,
    }

    struct StockOptions {
        menu: Menu,
        boot_policy: BootPolicy,
        cache_mode: CacheMode,
    }

    struct StockIdentity {
        boot_area: [u8; 32],
        original_footer: [u8; 32],
    }

    struct InstalledWad {
        title_id: u64,
        region: u16,
    }

    unsafe extern "C++" {
        include!("ffi.hpp");

        fn extract_seed(nand: &[u8], sidecar_keys: &[u8]) -> Result<String>;
        fn create_stock(
            keys: &[u8],
            boot_blocks: &[u8],
            bad_blocks: &[u16],
            setting_txt: &[u8],
            nand: &mut [u8],
            nus_cache_dir: &str,
            options: &StockOptions,
        ) -> Result<StockIdentity>;
        fn apply_system_settings(nand: &mut [u8], settings: &SystemSettings) -> Result<()>;
        fn install_nus_title(
            nand: &mut [u8],
            nus_cache_dir: &str,
            title_id: u64,
            version: u16,
            cache_mode: CacheMode,
        ) -> Result<()>;
        fn install_wad(
            nand: &mut [u8],
            wad_bytes: &[u8],
            expected_sha256: &[u8; 32],
            policy: WadPolicy,
        ) -> Result<InstalledWad>;
        fn remove_title(nand: &mut [u8], title_id: u64) -> Result<()>;
        fn apply_priiloader(
            nand: &mut [u8],
            app_bytes: &[u8],
            hacks_hash_ini: &[u8],
            settings: &PriiloaderSettings,
            hacks: &[HackOverride],
        ) -> Result<()>;
        fn prepare_backupmii(nand: &mut [u8], expected: &StockIdentity) -> Result<Vec<u8>>;
    }
}
