use eframe::{self, Frame as EFrame, NativeOptions};
use egui::{CentralPanel, ComboBox, Frame, Id, ScrollArea, Stroke, TextEdit, Ui};
use rfd::FileDialog;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EXECUTABLE_EXTENSIONS: &[&str] = &["exe", "bat", "cmd", "com", "pif", "vbs", "wsf"];
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPOSITORY: Option<&str> = option_env!("EMULATOR_HUB_GITHUB_REPOSITORY");

fn app_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/eframe_icon.png")).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Main,
    Settings,
}

#[derive(Debug, Clone)]
struct LaunchSettings {
    fullscreen_enabled: bool,
    fullscreen_arg: String,
    borderless_enabled: bool,
    borderless_arg: String,
    resolution_enabled: bool,
    resolution_width: u32,
    resolution_height: u32,
    res_width_arg: String,
    res_height_arg: String,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            fullscreen_enabled: false,
            fullscreen_arg: "-f".to_string(),
            borderless_enabled: false,
            borderless_arg: "-borderless".to_string(),
            resolution_enabled: false,
            resolution_width: 1920,
            resolution_height: 1080,
            res_width_arg: "-width".to_string(),
            res_height_arg: "-height".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct Platform {
    id: i64,
    name: String,
}

#[derive(Debug, Clone)]
struct Emulator {
    id: i64,
    name: String,
    executable_path: String,
    platform_id: i64,
    platform_name: String,
    settings: LaunchSettings,
}

#[derive(Debug, Clone)]
struct Rom {
    id: i64,
    title: String,
    source_path: String,
    platform_id: i64,
    platform_name: String,
    favorite: bool,
    last_played_at: Option<i64>,
    play_count: i64,
}

#[derive(Debug, Clone)]
struct AvailableUpdate {
    version: String,
    download_url: String,
    checksum: String,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Clone)]
struct StoredState {
    home_dir: Option<PathBuf>,
    current_dir: Option<PathBuf>,
    emulator_dir: Option<PathBuf>,
    default_settings: LaunchSettings,
    tab: Tab,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct LegacyConfig {
    home_dir: Option<String>,
    current_dir: Option<String>,
    emulator_dir: Option<String>,
    fullscreen_enabled: bool,
    fullscreen_arg: String,
    borderless_enabled: bool,
    borderless_arg: String,
    resolution_enabled: bool,
    resolution_width: u32,
    resolution_height: u32,
    res_width_arg: String,
    res_height_arg: String,
}

struct App {
    // SQLite access is intentionally synchronous on the UI thread for now. If
    // scanning moves to a worker thread, wrap this connection in a Mutex or
    // replace it with a small connection pool because Connection is not
    // Send/Sync.
    db: Connection,
    current_dir: PathBuf,
    home_dir: Option<PathBuf>,
    emulator_dir: Option<PathBuf>,
    status: String,
    tab: Tab,
    default_settings: LaunchSettings,
    platforms: Vec<Platform>,
    emulators: Vec<Emulator>,
    roms: Vec<Rom>,
    platform_input: String,
    selected_emulator_id: Option<i64>,
    editing_settings: LaunchSettings,
    pending_update: Option<AvailableUpdate>,
}

fn database_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    base.join("EmulatorHub").join("library.sqlite3")
}

fn legacy_config_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("emulator_hub_config.json");
        }
    }
    std::env::current_dir()
        .unwrap_or_default()
        .join("emulator_hub_config.json")
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn normalized_path_key(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let normalized = fs::canonicalize(&absolute).unwrap_or(absolute);
    let mut value = path_string(&normalized);
    if cfg!(windows) {
        value = value.to_lowercase();
    }
    value
}

fn is_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            EXECUTABLE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        .unwrap_or(false)
}

fn file_metadata(path: &Path) -> (Option<i64>, Option<i64>) {
    let Ok(metadata) = fs::metadata(path) else {
        return (None, None);
    };
    let size = i64::try_from(metadata.len()).ok();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64);
    (size, modified)
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn platform_name_or_unknown(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn init_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS app_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            home_dir TEXT,
            current_dir TEXT,
            emulator_dir TEXT,
            selected_tab TEXT NOT NULL DEFAULT 'Main',
            legacy_config_migrated INTEGER NOT NULL DEFAULT 0,
            default_fullscreen_enabled INTEGER NOT NULL DEFAULT 0,
            default_fullscreen_arg TEXT NOT NULL DEFAULT '-f',
            default_borderless_enabled INTEGER NOT NULL DEFAULT 0,
            default_borderless_arg TEXT NOT NULL DEFAULT '-borderless',
            default_resolution_enabled INTEGER NOT NULL DEFAULT 0,
            default_resolution_width INTEGER NOT NULL DEFAULT 1920,
            default_resolution_height INTEGER NOT NULL DEFAULT 1080,
            default_res_width_arg TEXT NOT NULL DEFAULT '-width',
            default_res_height_arg TEXT NOT NULL DEFAULT '-height'
        );

        INSERT OR IGNORE INTO app_state (id) VALUES (1);

        CREATE TABLE IF NOT EXISTS platforms (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS emulators (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            executable_path TEXT NOT NULL,
            executable_path_key TEXT NOT NULL UNIQUE,
            platform_id INTEGER NOT NULL REFERENCES platforms(id),
            fullscreen_enabled INTEGER NOT NULL DEFAULT 0,
            fullscreen_arg TEXT NOT NULL DEFAULT '-f',
            borderless_enabled INTEGER NOT NULL DEFAULT 0,
            borderless_arg TEXT NOT NULL DEFAULT '-borderless',
            resolution_enabled INTEGER NOT NULL DEFAULT 0,
            resolution_width INTEGER NOT NULL DEFAULT 1920,
            resolution_height INTEGER NOT NULL DEFAULT 1080,
            res_width_arg TEXT NOT NULL DEFAULT '-width',
            res_height_arg TEXT NOT NULL DEFAULT '-height',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS roms (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            source_path TEXT NOT NULL,
            source_path_key TEXT NOT NULL UNIQUE,
            platform_id INTEGER NOT NULL REFERENCES platforms(id),
            cover_art_path TEXT,
            favorite INTEGER NOT NULL DEFAULT 0,
            last_played_at INTEGER,
            play_count INTEGER NOT NULL DEFAULT 0,
            file_size INTEGER,
            modified_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        ",
    )
}

fn read_legacy_config() -> Result<LegacyConfig, Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(legacy_config_path())?;
    Ok(serde_json::from_str(&contents)?)
}

fn migrate_legacy_config(connection: &Connection) -> rusqlite::Result<String> {
    let already_migrated: i64 = connection.query_row(
        "SELECT legacy_config_migrated FROM app_state WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    if already_migrated != 0 {
        return Ok(String::new());
    }

    let legacy_path = legacy_config_path();
    let mut status = String::new();
    if legacy_path.exists() {
        match read_legacy_config() {
            Ok(config) => {
                let defaults = LaunchSettings {
                    fullscreen_enabled: config.fullscreen_enabled,
                    fullscreen_arg: if config.fullscreen_arg.is_empty() {
                        "-f".to_string()
                    } else {
                        config.fullscreen_arg
                    },
                    borderless_enabled: config.borderless_enabled,
                    borderless_arg: if config.borderless_arg.is_empty() {
                        "-borderless".to_string()
                    } else {
                        config.borderless_arg
                    },
                    resolution_enabled: config.resolution_enabled,
                    resolution_width: if config.resolution_width == 0 {
                        1920
                    } else {
                        config.resolution_width
                    },
                    resolution_height: if config.resolution_height == 0 {
                        1080
                    } else {
                        config.resolution_height
                    },
                    res_width_arg: if config.res_width_arg.is_empty() {
                        "-width".to_string()
                    } else {
                        config.res_width_arg
                    },
                    res_height_arg: if config.res_height_arg.is_empty() {
                        "-height".to_string()
                    } else {
                        config.res_height_arg
                    },
                };
                connection.execute(
                    "
                    UPDATE app_state SET
                        home_dir = ?1,
                        current_dir = ?2,
                        emulator_dir = ?3,
                        default_fullscreen_enabled = ?4,
                        default_fullscreen_arg = ?5,
                        default_borderless_enabled = ?6,
                        default_borderless_arg = ?7,
                        default_resolution_enabled = ?8,
                        default_resolution_width = ?9,
                        default_resolution_height = ?10,
                        default_res_width_arg = ?11,
                        default_res_height_arg = ?12,
                        legacy_config_migrated = 1
                    WHERE id = 1
                    ",
                    params![
                        config.home_dir,
                        config.current_dir,
                        config.emulator_dir,
                        defaults.fullscreen_enabled as i64,
                        defaults.fullscreen_arg,
                        defaults.borderless_enabled as i64,
                        defaults.borderless_arg,
                        defaults.resolution_enabled as i64,
                        defaults.resolution_width as i64,
                        defaults.resolution_height as i64,
                        defaults.res_width_arg,
                        defaults.res_height_arg,
                    ],
                )?;
                status = format!(
                    "Migrated legacy settings. Kept {} as a backup.",
                    legacy_path.display()
                );
            }
            Err(error) => {
                connection.execute(
                    "UPDATE app_state SET legacy_config_migrated = 1 WHERE id = 1",
                    [],
                )?;
                status = format!(
                    "Could not read legacy settings ({}). Kept {} as a backup.",
                    error,
                    legacy_path.display()
                );
            }
        }
    } else {
        connection.execute(
            "UPDATE app_state SET legacy_config_migrated = 1 WHERE id = 1",
            [],
        )?;
    }
    Ok(status)
}

fn open_database() -> Result<(Connection, String), Box<dyn Error + Send + Sync>> {
    let path = database_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    init_schema(&connection)?;
    let migration_status = migrate_legacy_config(&connection)?;
    Ok((connection, migration_status))
}

fn read_stored_state(connection: &Connection) -> rusqlite::Result<StoredState> {
    connection.query_row(
        "
        SELECT home_dir, current_dir, emulator_dir, selected_tab,
               default_fullscreen_enabled, default_fullscreen_arg,
               default_borderless_enabled, default_borderless_arg,
               default_resolution_enabled, default_resolution_width,
               default_resolution_height, default_res_width_arg,
               default_res_height_arg
        FROM app_state WHERE id = 1
        ",
        [],
        |row| {
            let selected_tab: String = row.get(3)?;
            Ok(StoredState {
                home_dir: row.get::<_, Option<String>>(0)?.map(PathBuf::from),
                current_dir: row.get::<_, Option<String>>(1)?.map(PathBuf::from),
                emulator_dir: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                tab: if selected_tab == "Settings" {
                    Tab::Settings
                } else {
                    Tab::Main
                },
                default_settings: LaunchSettings {
                    fullscreen_enabled: row.get::<_, i64>(4)? != 0,
                    fullscreen_arg: row.get(5)?,
                    borderless_enabled: row.get::<_, i64>(6)? != 0,
                    borderless_arg: row.get(7)?,
                    resolution_enabled: row.get::<_, i64>(8)? != 0,
                    resolution_width: row.get::<_, i64>(9)?.max(1) as u32,
                    resolution_height: row.get::<_, i64>(10)?.max(1) as u32,
                    res_width_arg: row.get(11)?,
                    res_height_arg: row.get(12)?,
                },
            })
        },
    )
}

fn load_platforms(connection: &Connection) -> rusqlite::Result<Vec<Platform>> {
    let mut statement = connection.prepare("SELECT id, name FROM platforms ORDER BY name")?;
    let rows = statement.query_map([], |row| {
        Ok(Platform {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    rows.collect()
}

fn load_emulators(connection: &Connection) -> rusqlite::Result<Vec<Emulator>> {
    let mut statement = connection.prepare(
        "
        SELECT e.id, e.name, e.executable_path, e.platform_id, p.name,
               e.fullscreen_enabled, e.fullscreen_arg,
               e.borderless_enabled, e.borderless_arg,
               e.resolution_enabled, e.resolution_width,
               e.resolution_height, e.res_width_arg, e.res_height_arg
        FROM emulators e
        JOIN platforms p ON p.id = e.platform_id
        ORDER BY e.name COLLATE NOCASE
        ",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(Emulator {
            id: row.get(0)?,
            name: row.get(1)?,
            executable_path: row.get(2)?,
            platform_id: row.get(3)?,
            platform_name: row.get(4)?,
            settings: LaunchSettings {
                fullscreen_enabled: row.get::<_, i64>(5)? != 0,
                fullscreen_arg: row.get(6)?,
                borderless_enabled: row.get::<_, i64>(7)? != 0,
                borderless_arg: row.get(8)?,
                resolution_enabled: row.get::<_, i64>(9)? != 0,
                resolution_width: row.get::<_, i64>(10)?.max(1) as u32,
                resolution_height: row.get::<_, i64>(11)?.max(1) as u32,
                res_width_arg: row.get(12)?,
                res_height_arg: row.get(13)?,
            },
        })
    })?;
    rows.collect()
}

fn load_roms(connection: &Connection) -> rusqlite::Result<Vec<Rom>> {
    let mut statement = connection.prepare(
        "
        SELECT r.id, r.title, r.source_path, r.platform_id, p.name,
               r.favorite, r.last_played_at, r.play_count
        FROM roms r
        JOIN platforms p ON p.id = r.platform_id
        ORDER BY r.favorite DESC, r.title COLLATE NOCASE
        ",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(Rom {
            id: row.get(0)?,
            title: row.get(1)?,
            source_path: row.get(2)?,
            platform_id: row.get(3)?,
            platform_name: row.get(4)?,
            favorite: row.get::<_, i64>(5)? != 0,
            last_played_at: row.get(6)?,
            play_count: row.get(7)?,
        })
    })?;
    rows.collect()
}

fn get_or_create_platform(
    transaction: &Transaction<'_>,
    name: &str,
    timestamp: i64,
) -> rusqlite::Result<i64> {
    transaction.execute(
        "INSERT OR IGNORE INTO platforms (name, created_at) VALUES (?1, ?2)",
        params![name, timestamp],
    )?;
    transaction.query_row(
        "SELECT id FROM platforms WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
}

fn enumerate_files(folder: &Path) -> Vec<PathBuf> {
    fs::read_dir(folder)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect()
}

fn version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim().trim_start_matches('v');
    let mut parts = version.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.split('-').next()?.parse().ok()?,
    ))
}

fn download_bytes(url: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let response = ureq::get(url).set("User-Agent", "EmulatorHub").call()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn fetch_latest_update() -> Result<Option<AvailableUpdate>, Box<dyn Error + Send + Sync>> {
    let Some(repository) =
        GITHUB_REPOSITORY.filter(|value| !value.is_empty() && !value.contains("OWNER/REPOSITORY"))
    else {
        return Err("This build has no GitHub repository configured for updates.".into());
    };

    let endpoint = format!("https://api.github.com/repos/{repository}/releases/latest");
    let release: GithubRelease = ureq::get(&endpoint)
        .set("User-Agent", "EmulatorHub")
        .call()?
        .into_json()?;
    if release.draft || release.prerelease {
        return Ok(None);
    }

    let Some(current_version) = version_tuple(APP_VERSION) else {
        return Err(format!("Invalid current version: {APP_VERSION}").into());
    };
    let Some(remote_version) = version_tuple(&release.tag_name) else {
        return Err(format!("Invalid release version: {}", release.tag_name).into());
    };
    if remote_version <= current_version {
        return Ok(None);
    }

    // Releases also contain the Setup.exe installer. The updater must select
    // the portable application executable, not the installer, because it
    // replaces the currently running binary in place.
    let expected_executable = format!("EmulatorHub-{}-windows-x86_64.exe", release.tag_name);
    let Some(executable) = release
        .assets
        .iter()
        .find(|asset| asset.name == expected_executable)
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.ends_with("-windows-x86_64.exe"))
        })
    else {
        return Err("The latest release has no Windows executable asset.".into());
    };

    let checksum_asset_name = format!("{}.sha256", executable.name);
    let checksum = if let Some(asset) = release
        .assets
        .iter()
        .find(|asset| asset.name == checksum_asset_name)
    {
        let contents = String::from_utf8(download_bytes(&asset.browser_download_url)?)?;
        contents
            .split_whitespace()
            .next()
            .map(str::to_lowercase)
            .ok_or_else(|| "The release checksum file is empty.".to_string())?
    } else if let Some(digest) = &executable.digest {
        digest
            .strip_prefix("sha256:")
            .unwrap_or(digest)
            .to_lowercase()
    } else {
        return Err("The latest release has no SHA-256 checksum.".into());
    };

    Ok(Some(AvailableUpdate {
        version: release.tag_name,
        download_url: executable.browser_download_url.clone(),
        checksum,
        notes: release.body,
    }))
}

fn apply_update_helper(args: &[String]) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() != 4 {
        return Err("Invalid update helper arguments.".into());
    }

    let target = PathBuf::from(&args[2]);
    let downloaded = PathBuf::from(&args[3]);
    let backup = target.with_extension("old");

    for _ in 0..120 {
        let moved_old = if target.exists() {
            let _ = fs::remove_file(&backup);
            fs::rename(&target, &backup).is_ok()
        } else {
            true
        };

        if moved_old {
            match fs::rename(&downloaded, &target) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup);
                    Command::new(&target).spawn()?;
                    return Ok(());
                }
                Err(error) => {
                    if backup.exists() {
                        let _ = fs::rename(&backup, &target);
                    }
                    return Err(error.into());
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    Err("Timed out waiting for Emulator Hub to exit.".into())
}

impl App {
    fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let (db, migration_status) = open_database()?;
        let stored_state = read_stored_state(&db)?;
        let current_dir = stored_state
            .current_dir
            .clone()
            .or_else(|| stored_state.home_dir.clone())
            .unwrap_or_default();
        let default_settings = stored_state.default_settings.clone();
        let mut app = Self {
            db,
            current_dir,
            home_dir: stored_state.home_dir,
            emulator_dir: stored_state.emulator_dir,
            status: if migration_status.is_empty() {
                "Library loaded.".to_string()
            } else {
                migration_status
            },
            tab: stored_state.tab,
            default_settings: default_settings.clone(),
            platforms: Vec::new(),
            emulators: Vec::new(),
            roms: Vec::new(),
            platform_input: "Unknown".to_string(),
            selected_emulator_id: None,
            editing_settings: default_settings,
            pending_update: None,
        };
        app.refresh_from_db()?;
        Ok(app)
    }

    fn refresh_from_db(&mut self) -> rusqlite::Result<()> {
        self.platforms = load_platforms(&self.db)?;
        self.emulators = load_emulators(&self.db)?;
        self.roms = load_roms(&self.db)?;
        if let Some(selected_id) = self.selected_emulator_id {
            if let Some(emulator) = self.emulators.iter().find(|item| item.id == selected_id) {
                self.editing_settings = emulator.settings.clone();
            } else {
                self.selected_emulator_id = None;
                self.editing_settings = self.default_settings.clone();
            }
        }
        Ok(())
    }

    fn save_app_state(&self) -> rusqlite::Result<()> {
        let tab = match self.tab {
            Tab::Main => "Main",
            Tab::Settings => "Settings",
        };
        let current_dir =
            (!self.current_dir.as_os_str().is_empty()).then(|| path_string(&self.current_dir));
        self.db.execute(
            "
            UPDATE app_state SET
                home_dir = ?1,
                current_dir = ?2,
                emulator_dir = ?3,
                selected_tab = ?4,
                default_fullscreen_enabled = ?5,
                default_fullscreen_arg = ?6,
                default_borderless_enabled = ?7,
                default_borderless_arg = ?8,
                default_resolution_enabled = ?9,
                default_resolution_width = ?10,
                default_resolution_height = ?11,
                default_res_width_arg = ?12,
                default_res_height_arg = ?13
            WHERE id = 1
            ",
            params![
                self.home_dir.as_ref().map(|path| path_string(path)),
                current_dir,
                self.emulator_dir.as_ref().map(|path| path_string(path)),
                tab,
                self.default_settings.fullscreen_enabled as i64,
                self.default_settings.fullscreen_arg,
                self.default_settings.borderless_enabled as i64,
                self.default_settings.borderless_arg,
                self.default_settings.resolution_enabled as i64,
                self.default_settings.resolution_width as i64,
                self.default_settings.resolution_height as i64,
                self.default_settings.res_width_arg,
                self.default_settings.res_height_arg,
            ],
        )?;
        Ok(())
    }

    fn import_rom_paths(&mut self, paths: Vec<PathBuf>) {
        let platform_name = platform_name_or_unknown(&self.platform_input);
        let timestamp = now_timestamp();
        let result = (|| -> rusqlite::Result<usize> {
            let transaction = self.db.transaction()?;
            let platform_id = get_or_create_platform(&transaction, &platform_name, timestamp)?;
            let mut imported = 0;
            for path in paths {
                if !path.is_file() || is_executable(&path) {
                    continue;
                }
                let (file_size, modified_at) = file_metadata(&path);
                transaction.execute(
                    "
                    INSERT INTO roms (
                        title, source_path, source_path_key, platform_id,
                        file_size, modified_at, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                    ON CONFLICT(source_path_key) DO UPDATE SET
                        title = excluded.title,
                        source_path = excluded.source_path,
                        platform_id = excluded.platform_id,
                        file_size = excluded.file_size,
                        modified_at = excluded.modified_at,
                        updated_at = excluded.updated_at
                    ",
                    params![
                        title_from_path(&path),
                        path_string(&path),
                        normalized_path_key(&path),
                        platform_id,
                        file_size,
                        modified_at,
                        timestamp,
                    ],
                )?;
                imported += 1;
            }
            transaction.commit()?;
            Ok(imported)
        })();
        match result {
            Ok(imported) => {
                let _ = self.save_app_state();
                let _ = self.refresh_from_db();
                self.status = format!("Imported or updated {imported} ROM(s).");
            }
            Err(error) => self.status = format!("ROM import failed: {error}"),
        }
    }

    fn import_emulator_paths(&mut self, paths: Vec<PathBuf>) {
        let platform_name = platform_name_or_unknown(&self.platform_input);
        let timestamp = now_timestamp();
        let defaults = self.default_settings.clone();
        let result = (|| -> rusqlite::Result<usize> {
            let transaction = self.db.transaction()?;
            let platform_id = get_or_create_platform(&transaction, &platform_name, timestamp)?;
            let mut imported = 0;
            for path in paths {
                if !path.is_file() || !is_executable(&path) {
                    continue;
                }
                transaction.execute(
                    "
                    INSERT INTO emulators (
                        name, executable_path, executable_path_key, platform_id,
                        fullscreen_enabled, fullscreen_arg,
                        borderless_enabled, borderless_arg,
                        resolution_enabled, resolution_width, resolution_height,
                        res_width_arg, res_height_arg, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
                    ON CONFLICT(executable_path_key) DO UPDATE SET
                        name = excluded.name,
                        executable_path = excluded.executable_path,
                        platform_id = excluded.platform_id,
                        updated_at = excluded.updated_at
                    ",
                    params![
                        title_from_path(&path),
                        path_string(&path),
                        normalized_path_key(&path),
                        platform_id,
                        defaults.fullscreen_enabled as i64,
                        defaults.fullscreen_arg,
                        defaults.borderless_enabled as i64,
                        defaults.borderless_arg,
                        defaults.resolution_enabled as i64,
                        defaults.resolution_width as i64,
                        defaults.resolution_height as i64,
                        defaults.res_width_arg,
                        defaults.res_height_arg,
                        timestamp,
                    ],
                )?;
                imported += 1;
            }
            transaction.commit()?;
            Ok(imported)
        })();
        match result {
            Ok(imported) => {
                let _ = self.refresh_from_db();
                self.status = format!("Imported or updated {imported} emulator(s).");
            }
            Err(error) => self.status = format!("Emulator import failed: {error}"),
        }
    }

    fn import_rom_folder(&mut self) {
        if let Some(folder) = FileDialog::new().pick_folder() {
            self.current_dir = folder.clone();
            if self.home_dir.is_none() {
                self.home_dir = Some(folder.clone());
            }
            self.import_rom_paths(enumerate_files(&folder));
        }
    }

    fn import_emulator_folder(&mut self) {
        if let Some(folder) = FileDialog::new().pick_folder() {
            self.emulator_dir = Some(folder.clone());
            self.import_emulator_paths(enumerate_files(&folder));
            let _ = self.save_app_state();
        }
    }

    fn launch_rom(&mut self, rom_id: i64) {
        let Some(rom) = self.roms.iter().find(|item| item.id == rom_id).cloned() else {
            return;
        };
        let Some(emulator) = self
            .emulators
            .iter()
            .find(|item| item.platform_id == rom.platform_id)
            .cloned()
        else {
            self.status = format!(
                "No emulator is configured for platform '{}'.",
                rom.platform_name
            );
            return;
        };
        if !Path::new(&rom.source_path).exists() {
            self.status = format!("ROM file is missing: {}", rom.source_path);
            return;
        }
        if !Path::new(&emulator.executable_path).exists() {
            self.status = format!("Emulator file is missing: {}", emulator.executable_path);
            return;
        }

        let mut command = Command::new(&emulator.executable_path);
        command.arg(&rom.source_path);
        append_launch_arguments(&mut command, &emulator.settings);
        match command.spawn() {
            Ok(_) => {
                let timestamp = now_timestamp();
                if let Err(error) = self.db.execute(
                    "
                    UPDATE roms
                    SET last_played_at = ?1, play_count = play_count + 1, updated_at = ?1
                    WHERE id = ?2
                    ",
                    params![timestamp, rom.id],
                ) {
                    self.status = format!("Launched ROM, but could not save play history: {error}");
                } else {
                    let _ = self.refresh_from_db();
                    self.status = format!("Launched '{}' with '{}'.", rom.title, emulator.name);
                }
            }
            Err(error) => self.status = format!("Failed to launch ROM: {error}"),
        }
    }

    fn toggle_favorite(&mut self, rom_id: i64) {
        match self.db.execute(
            "UPDATE roms SET favorite = CASE favorite WHEN 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2",
            params![now_timestamp(), rom_id],
        ) {
            Ok(_) => {
                let _ = self.refresh_from_db();
            }
            Err(error) => self.status = format!("Could not update favorite: {error}"),
        }
    }

    fn check_for_updates(&mut self) {
        self.status = "Checking for updates...".to_string();
        match fetch_latest_update() {
            Ok(Some(update)) => {
                self.status = format!("Version {} is available.", update.version);
                self.pending_update = Some(update);
            }
            Ok(None) => {
                self.pending_update = None;
                self.status = format!("Emulator Hub {APP_VERSION} is up to date.");
            }
            Err(error) => {
                self.pending_update = None;
                self.status = format!("Update check failed: {error}");
            }
        }
    }

    fn install_update(&mut self) {
        let Some(update) = self.pending_update.clone() else {
            return;
        };

        let result = (|| -> Result<(), Box<dyn Error + Send + Sync>> {
            self.status = format!("Downloading version {}...", update.version);
            let bytes = download_bytes(&update.download_url)?;
            let actual_checksum = format!("{:x}", Sha256::digest(&bytes));
            if !actual_checksum.eq_ignore_ascii_case(&update.checksum) {
                return Err("Downloaded update failed SHA-256 verification.".into());
            }

            let current_exe = std::env::current_exe()?;
            let pid = std::process::id();
            let downloaded = std::env::temp_dir().join(format!("EmulatorHub-update-{pid}.exe"));
            let helper = std::env::temp_dir().join(format!("EmulatorHub-updater-{pid}.exe"));
            fs::write(&downloaded, bytes)?;
            fs::copy(&current_exe, &helper)?;

            Command::new(&helper)
                .arg("--apply-update")
                .arg(&current_exe)
                .arg(&downloaded)
                .spawn()?;
            self.save_app_state()?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                eprintln!("Installing update and restarting Emulator Hub.");
                std::process::exit(0);
            }
            Err(error) => {
                self.status = format!("Update installation failed: {error}");
            }
        }
    }

    fn save_editing_settings(&mut self) {
        let settings = self.editing_settings.clone();
        let result = if let Some(emulator_id) = self.selected_emulator_id {
            self.db
                .execute(
                    "
                UPDATE emulators SET
                    fullscreen_enabled = ?1, fullscreen_arg = ?2,
                    borderless_enabled = ?3, borderless_arg = ?4,
                    resolution_enabled = ?5, resolution_width = ?6,
                    resolution_height = ?7, res_width_arg = ?8,
                    res_height_arg = ?9, updated_at = ?10
                WHERE id = ?11
                ",
                    params![
                        settings.fullscreen_enabled as i64,
                        settings.fullscreen_arg,
                        settings.borderless_enabled as i64,
                        settings.borderless_arg,
                        settings.resolution_enabled as i64,
                        settings.resolution_width as i64,
                        settings.resolution_height as i64,
                        settings.res_width_arg,
                        settings.res_height_arg,
                        now_timestamp(),
                        emulator_id,
                    ],
                )
                .map(|_| ())
        } else {
            self.default_settings = settings;
            self.save_app_state()
        };

        match result {
            Ok(_) => {
                let _ = self.refresh_from_db();
                self.status = if self.selected_emulator_id.is_some() {
                    "Emulator settings saved.".to_string()
                } else {
                    "Default launch settings saved.".to_string()
                };
            }
            Err(error) => self.status = format!("Could not save settings: {error}"),
        }
    }

    fn show_main_tab(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Platform:");
            ui.add(
                TextEdit::singleline(&mut self.platform_input)
                    .hint_text("e.g. NES, SNES, PlayStation"),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Import ROM Files").clicked() {
                if let Some(paths) = FileDialog::new().pick_files() {
                    self.import_rom_paths(paths);
                }
            }
            if ui.button("Import ROM Folder").clicked() {
                self.import_rom_folder();
            }
            if ui.button("Import Emulator File").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.import_emulator_paths(vec![path]);
                }
            }
            if ui.button("Import Emulator Folder").clicked() {
                self.import_emulator_folder();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Home").clicked() {
                if let Some(home) = &self.home_dir {
                    self.current_dir = home.clone();
                    let _ = self.save_app_state();
                }
            }
            if ui.button("Go Up").clicked() {
                if self.current_dir.pop() {
                    let _ = self.save_app_state();
                }
            }
            if ui.button("Refresh").clicked() {
                let _ = self.refresh_from_db();
            }
        });
        ui.separator();
        ui.label(&self.status);
        if !self.current_dir.as_os_str().is_empty() {
            ui.label(format!("Current directory: {}", self.current_dir.display()));
        }
        ui.separator();
        ui.heading(format!("ROM Library ({})", self.roms.len()));
        if self.roms.is_empty() {
            ui.label("No ROMs imported yet.");
        } else {
            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for rom in self.roms.clone() {
                    ui.horizontal(|ui| {
                        let missing = !Path::new(&rom.source_path).exists();
                        let label = if missing {
                            format!("{} [{}] — missing", rom.title, rom.platform_name)
                        } else {
                            format!("{} [{}]", rom.title, rom.platform_name)
                        };
                        if ui.button(label).clicked() {
                            self.launch_rom(rom.id);
                        }
                        if ui
                            .button(if rom.favorite { "★" } else { "☆" })
                            .on_hover_text("Toggle favorite")
                            .clicked()
                        {
                            self.toggle_favorite(rom.id);
                        }
                        ui.label(format!("Played: {}", rom.play_count));
                        if let Some(last_played_at) = rom.last_played_at {
                            ui.label(format!("Last played: {last_played_at}"));
                        }
                    });
                }
            });
        }
        ui.separator();
        if !self.platforms.is_empty() {
            ui.label(format!(
                "Platforms: {}",
                self.platforms
                    .iter()
                    .map(|platform| format!("{} ({})", platform.name, platform.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        ui.heading(format!("Emulators ({})", self.emulators.len()));
        for emulator in self.emulators.iter().take(8) {
            let missing = !Path::new(&emulator.executable_path).exists();
            ui.label(format!(
                "{} [{}]{}",
                emulator.name,
                emulator.platform_name,
                if missing { " — missing" } else { "" }
            ));
        }
    }

    fn show_settings_tab(&mut self, ui: &mut Ui) {
        ui.label("Imports use the platform field on the Main tab.");
        ui.separator();
        ui.label("Settings target:");
        let previous_target = self.selected_emulator_id;
        ComboBox::from_id_salt(Id::new("settings_emulator"))
            .selected_text(
                self.selected_emulator_id
                    .and_then(|id| self.emulators.iter().find(|item| item.id == id))
                    .map(|emulator| emulator.name.clone())
                    .unwrap_or_else(|| "Default launch settings".to_string()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.selected_emulator_id,
                    None,
                    "Default launch settings",
                );
                for emulator in self.emulators.clone() {
                    ui.selectable_value(
                        &mut self.selected_emulator_id,
                        Some(emulator.id),
                        format!("{} [{}]", emulator.name, emulator.platform_name),
                    );
                }
            });
        if previous_target != self.selected_emulator_id {
            self.editing_settings = self
                .selected_emulator_id
                .and_then(|id| self.emulators.iter().find(|item| item.id == id))
                .map(|emulator| emulator.settings.clone())
                .unwrap_or_else(|| self.default_settings.clone());
        }

        let settings = &mut self.editing_settings;
        ui.horizontal(|ui| {
            ui.label("Fullscreen:");
            ui.checkbox(&mut settings.fullscreen_enabled, "");
            ui.label("Argument:");
            ui.add(TextEdit::singleline(&mut settings.fullscreen_arg));
        });
        ui.horizontal(|ui| {
            ui.label("Borderless:");
            ui.checkbox(&mut settings.borderless_enabled, "");
            ui.label("Argument:");
            ui.add(TextEdit::singleline(&mut settings.borderless_arg));
        });
        ui.horizontal(|ui| {
            ui.label("Resolution:");
            ui.checkbox(&mut settings.resolution_enabled, "");
            ui.label("Width arg:");
            ui.add(TextEdit::singleline(&mut settings.res_width_arg));
            ui.label("Width:");
            ui.add(egui::DragValue::new(&mut settings.resolution_width));
            ui.label("Height arg:");
            ui.add(TextEdit::singleline(&mut settings.res_height_arg));
            ui.label("Height:");
            ui.add(egui::DragValue::new(&mut settings.resolution_height));
        });
        if ui.button("Save Settings").clicked() {
            self.save_editing_settings();
        }
        ui.separator();
        ui.heading(format!("Updates (current version {APP_VERSION})"));
        if ui.button("Check for Updates").clicked() {
            self.check_for_updates();
        }
        if let Some(update) = &self.pending_update {
            ui.label(format!("Version {} is available.", update.version));
            if !update.notes.trim().is_empty() {
                ui.label(update.notes.clone());
            }
            if ui.button("Download and Install Update").clicked() {
                self.install_update();
            }
        }
        ui.separator();
        ui.label("Emulator folder:");
        ui.label(
            self.emulator_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "Not set".to_string()),
        );
        if ui.button("Import Emulator Folder").clicked() {
            self.import_emulator_folder();
        }
    }
}

fn append_launch_arguments(command: &mut Command, settings: &LaunchSettings) {
    if settings.borderless_enabled {
        command.arg(&settings.borderless_arg);
    }
    if settings.fullscreen_enabled {
        command.arg(&settings.fullscreen_arg);
    }
    if settings.resolution_enabled {
        command.arg(&settings.res_width_arg);
        command.arg(settings.resolution_width.to_string());
        command.arg(&settings.res_height_arg);
        command.arg(settings.resolution_height.to_string());
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut EFrame) {
        CentralPanel::default().show(ui, |inner_ui| {
            Frame::new()
                .stroke(Stroke::new(1.0, egui::Color32::LIGHT_GRAY))
                .show(inner_ui, |inner_ui| {
                    inner_ui.horizontal(|inner_ui| {
                        inner_ui.label("Menu:");
                        ComboBox::from_id_salt(Id::new("tab_selector"))
                            .selected_text(match self.tab {
                                Tab::Main => "Main",
                                Tab::Settings => "Settings",
                            })
                            .show_ui(inner_ui, |ui| {
                                ui.selectable_value(&mut self.tab, Tab::Main, "Main");
                                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                            });
                    });
                    inner_ui.separator();
                    match self.tab {
                        Tab::Main => self.show_main_tab(inner_ui),
                        Tab::Settings => self.show_settings_tab(inner_ui),
                    }
                });
        });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.save_app_state();
    }
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--apply-update") {
        if let Err(error) = apply_update_helper(&args) {
            eprintln!("Update helper failed: {error}");
        }
        return Ok(());
    }

    let window_title = format!("Emulator Hub GUI v{APP_VERSION}");
    let mut options = NativeOptions::default();
    let mut viewport = options.viewport.with_title(window_title.clone());
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
    options.viewport = viewport;
    eframe::run_native(
        &window_title,
        options,
        Box::new(|_cc| {
            Ok(App::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| Box::new(std::io::Error::other(error.to_string())))?)
        }),
    )
}
