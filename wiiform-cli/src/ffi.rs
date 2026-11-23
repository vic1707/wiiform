// SPDX-License-Identifier: GPL-2.0-only
//! The CXX bridge to the Wiiform C++ library.
//!
//! The enums are the library's own (`wiiform/types.hpp`): CXX checks at compile
//! time that every variant here has the value it has in C++.
#[cxx::bridge(namespace = "wiiform::ffi")]
pub mod bridge {
    #[namespace = "wiiform"]
    #[repr(u16)]
    #[derive(Clone, Copy, Debug)]
    enum Menu {
        #[cxx_name = "e43"]
        E43 = 514,
        #[cxx_name = "u43"]
        U43 = 513,
        #[cxx_name = "j43"]
        J43 = 512,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum BootMiiState {
        #[cxx_name = "detected"]
        Detected = 0,
        #[cxx_name = "stock"]
        Stock = 1,
        #[cxx_name = "unknown"]
        Unknown = 2,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum Language {
        #[cxx_name = "ja"]
        Ja = 0,
        #[cxx_name = "en"]
        En = 1,
        #[cxx_name = "de"]
        De = 2,
        #[cxx_name = "fr"]
        Fr = 3,
        #[cxx_name = "es"]
        Es = 4,
        #[cxx_name = "it"]
        It = 5,
        #[cxx_name = "nl"]
        Nl = 6,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum Autoboot {
        #[cxx_name = "disabled"]
        Disabled = 0,
        #[cxx_name = "homebrew_channel"]
        HomebrewChannel = 1,
        #[cxx_name = "bootmii_ios"]
        BootmiiIos = 2,
        #[cxx_name = "system_menu"]
        SystemMenu = 3,
        #[cxx_name = "file"]
        File = 4,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum ReturnTo {
        #[cxx_name = "system_menu"]
        SystemMenu = 0,
        #[cxx_name = "priiloader"]
        Priiloader = 1,
        #[cxx_name = "autoboot"]
        Autoboot = 2,
        #[cxx_name = "file"]
        File = 3,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum ShutdownTo {
        #[cxx_name = "none"]
        None = 0,
        #[cxx_name = "priiloader"]
        Priiloader = 1,
        #[cxx_name = "autoboot"]
        Autoboot = 2,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum MountPoint {
        #[cxx_name = "automatic"]
        Auto = 0,
        #[cxx_name = "sd"]
        Sd = 1,
        #[cxx_name = "usb"]
        Usb = 2,
    }

    #[namespace = "wiiform"]
    #[repr(u8)]
    #[derive(Clone, Copy, Debug)]
    enum Hack {
        #[cxx_name = "fix_480p"]
        Fix480p = 0,
        #[cxx_name = "block_online_updates"]
        BlockOnlineUpdates = 1,
        #[cxx_name = "block_disc_updates"]
        BlockDiscUpdates = 2,
        #[cxx_name = "block_disc_autoboot"]
        BlockDiscAutoboot = 3,
        #[cxx_name = "auto_press_a"]
        AutoPressA = 4,
        #[cxx_name = "remove_deflicker"]
        RemoveDeflicker = 5,
        #[cxx_name = "move_disc_channel"]
        MoveDiscChannel = 6,
        #[cxx_name = "remove_nocopy_save_protection"]
        RemoveNocopySaveProtection = 7,
        #[cxx_name = "replace_health_screen"]
        ReplaceHealthScreen = 8,
        #[cxx_name = "no_background_music"]
        NoBackgroundMusic = 9,
        #[cxx_name = "wiimmfi"]
        Wiimmfi = 10,
        #[cxx_name = "region_free_everything"]
        RegionFreeEverything = 11,
        #[cxx_name = "no_sfx_and_background_music"]
        NoSfxAndBackgroundMusic = 12,
        #[cxx_name = "re_enable_bannerbomb"]
        ReEnableBannerbomb = 13,
        #[cxx_name = "usb_gecko_report"]
        UsbGeckoReport = 14,
        #[cxx_name = "usb_gecko_report_geckoos"]
        UsbGeckoReportGeckoOs = 15,
        #[cxx_name = "force_data_management"]
        ForceDataManagement = 16,
        #[cxx_name = "force_recovery_mode"]
        ForceRecoveryMode = 17,
        #[cxx_name = "remove_diagnostic_disc_check"]
        RemoveDiagnosticDiscCheck = 18,
        #[cxx_name = "no_delete_homebrew"]
        NoDeleteHomebrew = 19,
        #[cxx_name = "force_disc_ios248"]
        ForceDiscIos248 = 20,
        #[cxx_name = "force_disc_ios249"]
        ForceDiscIos249 = 21,
        #[cxx_name = "force_disc_ios250"]
        ForceDiscIos250 = 22,
        #[cxx_name = "force_disc_ios251"]
        ForceDiscIos251 = 23,
        #[cxx_name = "unblock_service_discs"]
        UnblockServiceDiscs = 24,
        #[cxx_name = "remove_ios16_disc_error"]
        RemoveIos16DiscError = 25,
        #[cxx_name = "mark_network_tested"]
        MarkNetworkTested = 26,
    }

    struct SystemSettings<'a> {
        nickname: &'a str,
        language: Language,
        country: u8,
        sensor_bar_above: bool,
        widescreen: bool,
        progressive: bool,
        pal60: bool,
    }

    struct PriiloaderSettings {
        autoboot: Autoboot,
        return_to: ReturnTo,
        shutdown_to: ShutdownTo,
        preferred_mount_point: MountPoint,
        stop_disc: bool,
        lid_slot_on_error: bool,
        ignore_shutdown_mode: bool,
        black_background: bool,
        dump_gecko_text: bool,
        show_rc_updates: bool,
    }

    struct StockIdentity {
        boot_area: [u8; 32],
        footer: [u8; 32],
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
        bootmii_boot2_version: String,
        bootmii_ios_tmd: Vec<u8>,
        bootmii_ios_ticket: Vec<u8>,
        bootmii_ios_content: Vec<u8>,
        wifi_config: Vec<u8>,
        wiimote_registrations: Vec<u8>,
    }

    struct CiosOptions {
        slot: u8,
        base: u8,
        revision: u16,
        replace_stock_stub: bool,
    }

    struct ContentPatch {
        cid: u32,
        offset: u32,
        original: Vec<u8>,
        replacement: Vec<u8>,
    }

    /// A position of -1 appends the module.
    struct AddedModule {
        cid: u32,
        position: i32,
        bytes: Vec<u8>,
    }

    struct CiosBuild {
        base_version: u16,
        options: CiosOptions,
        patches: Vec<ContentPatch>,
        modules: Vec<AddedModule>,
    }

    struct InstalledWad {
        title_id: u64,
        region: u16,
        version: u16,
        already_present: bool,
    }

    struct TitleVersion {
        id: u64,
        version: u16,
    }

    /// An empty directory means no cache: titles are downloaded into memory.
    struct NusCache<'a> {
        directory: &'a str,
        offline: bool,
    }

    struct RvloaderIos<'a> {
        source: TitleVersion,
        slot: u8,
        es_patches: bool,
        wifi_module: &'a [u8],
    }

    unsafe extern "C++" {
        include!("ffi.hpp");

        #[namespace = "wiiform"]
        type Menu;
        #[namespace = "wiiform"]
        type BootMiiState;
        #[namespace = "wiiform"]
        type Language;
        #[namespace = "wiiform"]
        type Autoboot;
        #[namespace = "wiiform"]
        type ReturnTo;
        #[namespace = "wiiform"]
        type ShutdownTo;
        #[namespace = "wiiform"]
        type MountPoint;
        #[namespace = "wiiform"]
        type Hack;

        fn extract_seed(nand: &[u8], sidecar_keys: &[u8]) -> Result<SeedParts>;
        fn stock_titles(menu: Menu) -> Result<Vec<TitleVersion>>;
        fn verify_stock(nand: &[u8], seed: &StockSeed<'_>, menu: Menu) -> Result<()>;
        fn verify_generated_nand(nand: &[u8], expected: &StockIdentity) -> Result<()>;

        fn write_backupmii_header(keys: &mut [u8]) -> Result<bool>;
        fn initialize_image(
            nand: &mut [u8],
            seed: &StockSeed<'_>,
            menu: Menu,
        ) -> Result<StockIdentity>;
        fn initialize_image_with_source_boot(
            nand: &mut [u8],
            seed: &StockSeed<'_>,
            menu: Menu,
        ) -> Result<StockIdentity>;
        fn install_nus_title(
            nand: &mut [u8],
            cache: &NusCache<'_>,
            title: TitleVersion,
        ) -> Result<InstalledWad>;
        fn install_rvloader_ios(
            nand: &mut [u8],
            cache: &NusCache<'_>,
            ios: &RvloaderIos<'_>,
        ) -> Result<InstalledWad>;
        fn set_rvloader_vga(nand: &mut [u8], enabled: bool) -> Result<()>;
        fn install_wad(nand: &mut [u8], wad: &[u8]) -> Result<InstalledWad>;
        fn install_channel(nand: &mut [u8], wad: &[u8]) -> Result<InstalledWad>;
        fn install_unsigned_channel(nand: &mut [u8], wad: &[u8]) -> Result<InstalledWad>;
        fn install_homebrew_channel(nand: &mut [u8], wad: &[u8]) -> Result<InstalledWad>;
        fn install_cios(nand: &mut [u8], wad: &[u8], options: &CiosOptions)
        -> Result<InstalledWad>;
        fn install_bootmii_ios(
            nand: &mut [u8],
            tmd: &[u8],
            ticket: &[u8],
            content: &[u8],
        ) -> Result<InstalledWad>;
        fn install_built_cios(
            nand: &mut [u8],
            cache: &NusCache<'_>,
            build: &CiosBuild,
        ) -> Result<InstalledWad>;
        fn remove_title(nand: &mut [u8], title_id: u64) -> Result<()>;
        fn apply_system_settings(
            nand: &mut [u8],
            settings: &SystemSettings<'_>,
            registrations: &[u8],
        ) -> Result<()>;
        fn apply_wifi_config(nand: &mut [u8], config: &[u8]) -> Result<()>;
        fn install_priiloader(nand: &mut [u8], app_bytes: &[u8]) -> Result<()>;
        fn install_priiloader_boot_file(
            nand: &mut [u8],
            dol: &[u8],
            hardware_access: bool,
        ) -> Result<()>;
        fn configure_priiloader(nand: &mut [u8], settings: &PriiloaderSettings) -> Result<()>;
        fn set_priiloader_hacks(
            nand: &mut [u8],
            hacks_hash_ini: &[u8],
            enabled_hacks: &[Hack],
        ) -> Result<()>;
    }
}
