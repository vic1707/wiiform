// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use wiiform::native::{self, BootPolicy, DisplaySource, SettingOverride};
use wiiform::worker::Worker;

#[derive(Debug, Parser)]
#[command(
    name = "wii-nand",
    version,
    about = "Inspect and prepare Wii NAND images"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize a fresh 4.3 image from a console seed.
    Create {
        #[arg(long, value_enum, default_value = "stock")]
        boot_policy: BootArgument,
        #[arg(long = "setting-override", value_parser = parse_override, action = ArgAction::Append)]
        settings: Vec<SettingOverride>,
        seed: PathBuf,
        new_nand: PathBuf,
    },
    /// Install one exact NUS title ID and version from the cache or network.
    InstallTitle {
        #[arg(long, required = true)]
        in_place: bool,
        #[arg(long)]
        offline: bool,
        work_nand: PathBuf,
        cache: PathBuf,
        title_id: TitleId,
        version: u16,
    },
    /// Validate a WAD and report its content hashes and signature state.
    InspectWad {
        #[arg(long)]
        allow_unverified_signatures: bool,
        #[arg(long)]
        sha256: Option<String>,
        /// Use `-` to read the WAD bytes from standard input.
        wad: PathBuf,
    },
    /// Install a WAD after checking its expected SHA-256.
    InstallWad {
        #[arg(long, required = true)]
        in_place: bool,
        #[arg(long)]
        allow_unverified_signatures: bool,
        #[arg(long, required = true)]
        sha256: String,
        work_nand: PathBuf,
        wad: PathBuf,
    },
    /// Remove one supported downloadable channel by its 16-digit title ID.
    RemoveTitle {
        #[arg(long, required = true)]
        in_place: bool,
        work_nand: PathBuf,
        title_id: TitleId,
    },
    /// Inspect installed display settings.
    InspectDisplay { nand: PathBuf },
    /// Inspect Wii setup and language settings in SYSCONF.
    InspectSysconf { nand: PathBuf },
    /// Inspect the installed Priiloader layout and settings.
    InspectPriiloader { nand: PathBuf },
    /// Export the recognized, pinned Priiloader app from an authenticated NAND.
    ExportPriiloaderApp { nand: PathBuf, new_app: PathBuf },
    /// Install the pinned Priiloader app and configure its settings.
    InstallPriiloader {
        #[arg(long = "setting-override", value_parser = parse_override, action = ArgAction::Append)]
        settings: Vec<SettingOverride>,
        nand: PathBuf,
        priiloader_app: PathBuf,
        new_nand: PathBuf,
    },
    /// Set Priiloader settings and optional supported system-menu hacks.
    SetPriiloader {
        #[arg(long = "setting-override", value_parser = parse_override, action = ArgAction::Append)]
        settings: Vec<SettingOverride>,
        #[arg(long)]
        hack_definitions: Option<PathBuf>,
        #[arg(long = "hack-override", value_parser = parse_override, action = ArgAction::Append)]
        hacks: Vec<SettingOverride>,
        nand: PathBuf,
        new_nand: PathBuf,
    },
    /// Set display fields in the NAND's SYSCONF.
    SetDisplay {
        #[arg(long, required = true)]
        in_place: bool,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long, value_parser = ["4:3", "16:9"])]
        aspect: Option<String>,
        #[arg(long, value_enum)]
        progressive: Option<OnOff>,
        #[arg(long, value_enum)]
        pal60: Option<OnOff>,
        work_nand: PathBuf,
    },
    /// Set setup, nickname, language, and display fields in SYSCONF.
    SetSysconf {
        #[arg(long, required = true)]
        in_place: bool,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        require_setup: bool,
        #[arg(long)]
        nickname: Option<String>,
        #[arg(long, value_parser = ["ja", "en", "de", "fr", "es", "it", "nl"])]
        language: Option<String>,
        #[arg(long, value_parser = ["4:3", "16:9"])]
        aspect: Option<String>,
        #[arg(long, value_enum)]
        progressive: Option<OnOff>,
        #[arg(long, value_enum)]
        pal60: Option<OnOff>,
        work_nand: PathBuf,
    },
    /// Print the pinned stock 4.3 title IDs and versions for an area.
    StockTitles {
        #[arg(value_parser = ["EUR", "USA", "JPN"])]
        area: String,
    },
    /// Check a seed and images on the host, writing a new report.
    Preflight {
        #[arg(long)]
        candidate_only: bool,
        #[arg(long, value_enum, default_value = "stock")]
        boot_policy: BootArgument,
        #[arg(long = "setting-override", value_parser = parse_override, action = ArgAction::Append)]
        settings: Vec<SettingOverride>,
        seed: PathBuf,
        /// REFERENCE CANDIDATE REPORT, or CANDIDATE REPORT with --candidate-only.
        #[arg(required = true, num_args = 2..=3)]
        paths: Vec<PathBuf>,
    },
    /// Write a new read-only filesystem inventory report.
    Inventory { nand: PathBuf, new_report: PathBuf },
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum BootArgument {
    #[default]
    Stock,
    PreserveSource,
}

impl From<BootArgument> for BootPolicy {
    fn from(value: BootArgument) -> Self {
        match value {
            BootArgument::Stock => Self::Stock,
            BootArgument::PreserveSource => Self::PreserveSource,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TitleId(u64);

impl FromStr for TitleId {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("title ID must contain exactly 16 hexadecimal digits".into());
        }
        let id = u64::from_str_radix(value, 16).map_err(|_| "invalid title ID")?;
        if id == 0 {
            return Err("title ID cannot be zero".into());
        }
        Ok(Self(id))
    }
}

fn parse_override(value: &str) -> std::result::Result<SettingOverride, String> {
    let (name, value) = value
        .split_once('=')
        .ok_or_else(|| "expected NAME=VALUE".to_owned())?;
    if name.is_empty() || value.is_empty() {
        return Err("expected non-empty NAME=VALUE".into());
    }
    Ok(SettingOverride {
        name: name.to_owned(),
        value: value.to_owned(),
    })
}

fn unique_overrides(values: Vec<SettingOverride>, label: &str) -> Result<Vec<SettingOverride>> {
    let mut names = std::collections::HashSet::with_capacity(values.len());
    for value in &values {
        if !names.insert(value.name.as_str()) {
            bail!("duplicate {label} override: {}", value.name);
        }
    }
    drop(names);
    Ok(values)
}

fn report(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn require_native_settings(settings: Vec<SettingOverride>) -> Result<Vec<SettingOverride>> {
    unique_overrides(settings, "setting")
}

fn display_overrides(
    aspect: Option<String>,
    progressive: Option<OnOff>,
    pal60: Option<OnOff>,
) -> Vec<SettingOverride> {
    let mut values = Vec::new();
    if let Some(aspect) = aspect {
        values.push(SettingOverride {
            name: "aspect".into(),
            value: aspect,
        });
    }
    if let Some(progressive) = progressive {
        values.push(SettingOverride {
            name: "progressive".into(),
            value: progressive.as_str().into(),
        });
    }
    if let Some(pal60) = pal60 {
        values.push(SettingOverride {
            name: "pal60".into(),
            value: pal60.as_str().into(),
        });
    }
    values
}

fn worker_inspect(worker: &Worker, result: Result<serde_json::Value>) -> Result<()> {
    report(result.with_context(|| {
        format!(
            "read-only inspection failed via {}",
            worker.executable().display()
        )
    })?)
}

fn execute(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Create {
            boot_policy,
            settings,
            seed,
            new_nand,
        } => {
            native::create(
                &seed,
                &new_nand,
                boot_policy.into(),
                &require_native_settings(settings)?,
            )?;
            println!("Wrote an initialized NAND working image.");
            Ok(0)
        }
        Command::InstallTitle {
            in_place: _,
            offline,
            work_nand,
            cache,
            title_id,
            version,
        } => {
            if version == u16::MAX {
                bail!("Install requires a concrete decimal version below 65535.");
            }
            native::install_nus_title(&work_nand, &cache, title_id.0, version, offline)?;
            Ok(0)
        }
        Command::InspectWad {
            allow_unverified_signatures,
            sha256,
            wad,
        } => {
            let worker = Worker::bundled()?;
            report(worker.inspect_wad(&wad, sha256.as_deref(), allow_unverified_signatures)?)?;
            Ok(0)
        }
        Command::InstallWad {
            in_place: _,
            allow_unverified_signatures,
            sha256,
            work_nand,
            wad,
        } => {
            native::install_wad(&work_nand, &wad, &sha256, allow_unverified_signatures)?;
            Ok(0)
        }
        Command::RemoveTitle {
            in_place: _,
            work_nand,
            title_id,
        } => {
            native::remove_title(&work_nand, title_id.0)?;
            Ok(0)
        }
        Command::InspectDisplay { nand } => {
            let worker = Worker::bundled()?;
            worker_inspect(&worker, worker.inspect_display(&nand))?;
            Ok(0)
        }
        Command::InspectSysconf { nand } => {
            let worker = Worker::bundled()?;
            worker_inspect(&worker, worker.inspect_sysconf(&nand))?;
            Ok(0)
        }
        Command::InspectPriiloader { nand } => {
            let worker = Worker::bundled()?;
            worker_inspect(&worker, worker.inspect_priiloader(&nand))?;
            Ok(0)
        }
        Command::ExportPriiloaderApp { nand, new_app } => {
            Worker::bundled()?.export_priiloader_app(&nand, &new_app)
        }
        Command::InstallPriiloader {
            settings,
            nand,
            priiloader_app,
            new_nand,
        } => {
            report(native::install_priiloader(
                &nand,
                &priiloader_app,
                &new_nand,
                &require_native_settings(settings)?,
            )?)?;
            Ok(0)
        }
        Command::SetPriiloader {
            settings,
            hack_definitions,
            hacks,
            nand,
            new_nand,
        } => {
            let settings = require_native_settings(settings)?;
            let hacks = unique_overrides(hacks, "hack")?;
            report(native::set_priiloader_settings(
                &nand,
                &new_nand,
                &settings,
                hack_definitions.as_deref(),
                &hacks,
            )?)?;
            Ok(0)
        }
        Command::SetDisplay {
            in_place: _,
            from,
            aspect,
            progressive,
            pal60,
            work_nand,
        } => {
            let source = match from {
                Some(path) => Worker::bundled()?.display_source(&path)?,
                None => DisplaySource::default(),
            };
            report(native::set_display(
                &work_nand,
                &source,
                &display_overrides(aspect, progressive, pal60),
            )?)?;
            Ok(0)
        }
        Command::SetSysconf {
            in_place: _,
            from,
            require_setup,
            nickname,
            language,
            aspect,
            progressive,
            pal60,
            work_nand,
        } => {
            let source = match from {
                Some(path) => Worker::bundled()?.display_source(&path)?,
                None => DisplaySource::default(),
            };
            let mut overrides = display_overrides(aspect, progressive, pal60);
            if require_setup {
                overrides.push(SettingOverride {
                    name: "require-setup".into(),
                    value: "true".into(),
                });
            }
            if let Some(nickname) = nickname {
                overrides.push(SettingOverride {
                    name: "nickname".into(),
                    value: nickname,
                });
            }
            if let Some(language) = language {
                overrides.push(SettingOverride {
                    name: "language".into(),
                    value: language,
                });
            }
            report(native::set_sysconf(&work_nand, &source, &overrides)?)?;
            Ok(0)
        }
        Command::StockTitles { area } => {
            let worker = Worker::bundled()?;
            report(worker.stock_titles(&area)?)?;
            Ok(0)
        }
        Command::Preflight {
            candidate_only,
            boot_policy,
            settings,
            seed,
            paths,
        } => {
            let expected_paths = if candidate_only { 2 } else { 3 };
            if paths.len() != expected_paths {
                bail!(
                    "{} requires {} path arguments after SEED",
                    if candidate_only {
                        "--candidate-only"
                    } else {
                        "preflight"
                    },
                    expected_paths
                );
            }
            let mut args = Vec::new();
            if candidate_only {
                args.push(OsString::from("--candidate-only"));
            }
            args.push(OsString::from("--boot-policy"));
            args.push(OsString::from(match boot_policy {
                BootArgument::Stock => "stock",
                BootArgument::PreserveSource => "preserve-source",
            }));
            for setting in require_native_settings(settings)? {
                args.push(OsString::from("--setting-override"));
                args.push(OsString::from(format!(
                    "{}={}",
                    setting.name, setting.value
                )));
            }
            args.push(seed.into_os_string());
            args.extend(paths.into_iter().map(PathBuf::into_os_string));
            Worker::bundled()?.passthrough(args)
        }
        Command::Inventory { nand, new_report } => Worker::bundled()?.passthrough([
            OsString::from("--inventory"),
            nand.into_os_string(),
            new_report.into_os_string(),
        ]),
    }
}

fn main() {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
