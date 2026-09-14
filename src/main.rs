use eframe::{self, Frame as EFrame, NativeOptions};
use egui::{
    Align, Align2, CentralPanel, Color32, ComboBox, FontId, Frame, Id, Layout, Panel, RichText,
    ScrollArea, Stroke, TextEdit, TextureHandle, Ui, Vec2, Visuals,
};
use image::ImageReader;
use rfd::FileDialog;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const EXECUTABLE_EXTENSIONS: &[&str] = &["exe", "bat", "cmd", "com", "pif", "vbs", "wsf"];
const MAGPIE_PRESETS: &[(&str, &str)] = &[
    ("FSR", "AMD FidelityFX Super Resolution"),
    ("Anime4K", "Anime4K anime/game upscaling"),
    ("CAS", "Contrast Adaptive Sharpening"),
    ("xBRZ", "Pixel-art focused scaling"),
    ("Lanczos", "High-quality traditional scaling"),
    ("Nearest", "Crisp nearest-neighbor scaling"),
];
// Cargo keeps the package at three-part semver while the display/release
// version can include a hotfix component.
const APP_VERSION: &str = "0.0.5-alpha1";
const GITHUB_REPOSITORY: Option<&str> = option_env!("EMULATOR_HUB_GITHUB_REPOSITORY");

fn app_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/eframe_icon.png")).ok()
}

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = APP_BACKGROUND;
    visuals.window_fill = PANEL_BACKGROUND;
    visuals.extreme_bg_color = Color32::from_rgb(7, 12, 17);
    visuals.faint_bg_color = Color32::from_rgb(14, 22, 30);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke.color = APP_BACKGROUND;
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.noninteractive.bg_fill = PANEL_BACKGROUND;
    visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(34, 58, 70);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke.color = APP_BACKGROUND;
    visuals.widgets.open.bg_fill = Color32::from_rgb(28, 47, 57);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(11.0, 7.0);
    style.visuals.override_text_color = Some(TEXT_PRIMARY);
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn card_frame(fill: Color32, stroke: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(12))
}

fn icon_label(value: &str) -> String {
    let words: Vec<Vec<char>> = value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();

    let label: String = if words.len() > 1 {
        words
            .iter()
            .take(3)
            .filter_map(|word| word.first().copied())
            .collect()
    } else {
        words
            .first()
            .map(|word| word.iter().take(3).collect())
            .unwrap_or_else(|| "?".to_string())
    };

    label.to_uppercase()
}

fn accent_for_name(name: &str) -> Color32 {
    const PALETTE: [Color32; 5] = [
        Color32::from_rgb(71, 203, 232),
        Color32::from_rgb(128, 157, 255),
        Color32::from_rgb(191, 157, 255),
        Color32::from_rgb(255, 188, 105),
        Color32::from_rgb(75, 221, 190),
    ];
    let hash = name
        .bytes()
        .fold(0u8, |value, byte| value.wrapping_mul(31).wrapping_add(byte));
    PALETTE[hash as usize % PALETTE.len()]
}

fn show_icon_tile(ui: &mut Ui, label: &str, accent: Color32, missing: bool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(46.0), egui::Sense::hover());
    let color = if missing { DANGER } else { accent };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(9), color.gamma_multiply(0.2));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(if label.len() > 2 { 12.0 } else { 15.0 }),
        color,
    );
    if missing {
        response.on_hover_text("The imported file or executable is missing");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Main,
    Upscaling,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryFilter {
    All,
    Favorites,
}

const APP_BACKGROUND: Color32 = Color32::from_rgb(10, 16, 23);
const PANEL_BACKGROUND: Color32 = Color32::from_rgb(17, 25, 34);
const PANEL_RAISED: Color32 = Color32::from_rgb(22, 32, 43);
const ACCENT: Color32 = Color32::from_rgb(71, 203, 232);
const ACCENT_GREEN: Color32 = Color32::from_rgb(75, 221, 190);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(235, 243, 248);
const TEXT_MUTED: Color32 = Color32::from_rgb(146, 165, 178);
const DANGER: Color32 = Color32::from_rgb(244, 117, 117);

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

#[allow(dead_code)]
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
    cover_art_path: Option<String>,
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
    artwork_api_key: String,
    default_settings: LaunchSettings,
    tab: Tab,
    magpie_enabled: bool,
    magpie_auto_start: bool,
    magpie_preset: String,
    magpie_scale: f32,
    magpie_sharpness: f32,
    magpie_auto_scale: bool,
    magpie_hotkey_key: String,
    magpie_auto_scale_delay_ms: u64,
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
    search_query: String,
    library_filter: LibraryFilter,
    selected_rom_id: Option<i64>,
    cover_art_textures: HashMap<String, TextureHandle>,
    emulator_icon_textures: HashMap<String, TextureHandle>,
    artwork_api_key: String,
    magpie_enabled: bool,
    magpie_auto_start: bool,
    magpie_preset: String,
    magpie_scale: f32,
    magpie_sharpness: f32,
    magpie_auto_scale: bool,
    magpie_hotkey_key: String,
    magpie_auto_scale_delay_ms: u64,
    magpie_process: Option<std::process::Child>,
    pending_auto_scale: Option<PendingAutoScale>,
}

#[derive(Debug, Clone)]
struct PendingAutoScale {
    process_id: u32,
    not_before: Instant,
    expires_at: Instant,
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

fn is_non_emulator_executable(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let has_sdl_suffix = stem
        .split(['.', '-', '_', ' '])
        .any(|part| matches!(part, "sdl" | "sdl2"));

    stem == "sdl"
        || stem.starts_with("sdl2")
        || has_sdl_suffix
        || stem.ends_with(".sdl")
        || stem.ends_with(".sdl2")
        || stem.starts_with("unins")
        || stem.starts_with("uninstall")
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

fn is_unknown_platform(name: &str) -> bool {
    name.trim().is_empty() || name.eq_ignore_ascii_case("unknown")
}

fn infer_rom_platform(path: &Path) -> Option<&'static str> {
    let name = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    // ISO files are used by several consoles. Recognize known PS2 titles
    // before falling back to the extension-based platform mapping.
    if name.contains("god hand") || name.contains("godhand") {
        return Some("PlayStation 2");
    }

    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let platform = match extension.as_str() {
        "gba" => "Game Boy Advance",
        "gbc" => "Game Boy Color",
        "gb" => "Game Boy",
        "nes" => "Nintendo Entertainment System",
        "smc" | "sfc" => "Super Nintendo",
        "n64" | "z64" | "v64" => "Nintendo 64",
        "nds" => "Nintendo DS",
        "3ds" | "cia" => "Nintendo 3DS",
        "iso" | "gcm" | "rvz" => "Nintendo GameCube",
        "wbfs" => "Nintendo Wii",
        "ps1" | "psx" | "pbp" => "PlayStation",
        "ps2" => "PlayStation 2",
        "cso" => "PlayStation Portable",
        "md" | "gen" | "smd" => "Sega Genesis",
        "sms" => "Sega Master System",
        "a26" => "Atari 2600",
        "pce" => "TurboGrafx-16",
        _ => {
            // Imported archives do not expose the ROM extension in their
            // filename. Keep the common Pokémon GBA archive names usable
            // without requiring the user to re-import them manually.
            if name.contains("pokemon")
                && [
                    "emerald",
                    "ruby",
                    "sapphire",
                    "fire red",
                    "firered",
                    "leaf green",
                    "leafgreen",
                ]
                .iter()
                .any(|title| name.contains(title))
            {
                "Game Boy Advance"
            } else {
                return None;
            }
        }
    };
    Some(platform)
}

fn is_ambiguous_rom_platform(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case("iso"))
        .unwrap_or(false)
}

fn infer_emulator_platform(name: &str, path: &Path) -> Option<&'static str> {
    let value = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        path.to_string_lossy().to_ascii_lowercase()
    );
    if value.contains("mgba") || value.contains("visualboy") {
        Some("Game Boy Advance")
    } else if value.contains("dolphin") {
        Some("Nintendo GameCube")
    } else if value.contains("pcsx2") {
        Some("PlayStation 2")
    } else if value.contains("duckstation") {
        Some("PlayStation")
    } else if value.contains("ppsspp") {
        Some("PlayStation Portable")
    } else if value.contains("desmume") || value.contains("melonds") {
        Some("Nintendo DS")
    } else if value.contains("citra") {
        Some("Nintendo 3DS")
    } else if value.contains("yuzu") || value.contains("ryujinx") {
        Some("Nintendo Switch")
    } else if value.contains("project64") {
        Some("Nintendo 64")
    } else if value.contains("snes9x") {
        Some("Super Nintendo")
    } else if value.contains("nestopia") {
        Some("Nintendo Entertainment System")
    } else if value.contains("gens") {
        Some("Sega Genesis")
    } else {
        None
    }
}

fn cover_art_path_for_rom(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy();
    let mut candidates = Vec::new();
    for extension in ["png", "jpg", "jpeg"] {
        candidates.push(parent.join(format!("{stem}.{extension}")));
        candidates.push(parent.join(format!("{stem}-cover.{extension}")));
        candidates.push(parent.join(format!("{stem}_cover.{extension}")));
        candidates.push(parent.join("covers").join(format!("{stem}.{extension}")));
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|candidate| path_string(&candidate))
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                vec![byte as char]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}

fn artwork_cache_directory() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    base.join("EmulatorHub").join("artwork")
}

fn cached_artwork_path(url: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let extension = url
        .split('?')
        .next()
        .and_then(|value| value.rsplit('.').next())
        .filter(|value| {
            matches!(
                (*value).to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg"
            )
        })
        .unwrap_or("jpg");
    artwork_cache_directory().join(format!("{hash}.{extension}"))
}

fn image_url_from_value(value: &serde_json::Value, base_url: Option<&str>) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let key_lower = key.to_ascii_lowercase();
                if key_lower.contains("front")
                    || key_lower.contains("boxart")
                    || key_lower == "image"
                    || key_lower == "url"
                    || key_lower == "filename"
                {
                    if let Some(raw) = child.as_str() {
                        if let Some(url) = normalize_image_url(raw, base_url) {
                            return Some(url);
                        }
                    }
                }
            }
            object
                .values()
                .find_map(|child| image_url_from_value(child, base_url))
        }
        serde_json::Value::Array(array) => array
            .iter()
            .find_map(|child| image_url_from_value(child, base_url)),
        _ => None,
    }
}

fn normalize_image_url(value: &str, base_url: Option<&str>) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(value.to_string());
    }
    if value.starts_with("//") {
        return Some(format!("https:{value}"));
    }
    base_url.map(|base| {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            value.trim_start_matches('/')
        )
    })
}

fn find_original_image_base_url(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(original) = object.get("original").and_then(|item| item.as_str()) {
                if original.starts_with("http://") || original.starts_with("https://") {
                    return Some(original.to_string());
                }
            }
            object.values().find_map(find_original_image_base_url)
        }
        serde_json::Value::Array(array) => array.iter().find_map(find_original_image_base_url),
        _ => None,
    }
}

fn artwork_search_terms(title: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let add_term = |terms: &mut Vec<String>, value: String| {
        let value = value.split(['(', '[']).next().unwrap_or(&value);
        let value = value.replace(" - ", " ").replace("  ", " ");
        let value = value.trim().to_string();
        if !value.is_empty() && !terms.iter().any(|term| term.eq_ignore_ascii_case(&value)) {
            terms.push(value);
        }
    };

    add_term(&mut terms, title.to_string());
    add_term(&mut terms, title.replace('-', " "));
    terms
}

fn first_game_id(value: &serde_json::Value) -> Option<i64> {
    value
        .get("data")?
        .get("games")?
        .as_array()?
        .first()?
        .get("id")?
        .as_i64()
}

fn fetch_artwork_url(
    api_key: &str,
    title: &str,
    _platform: &str,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    for search_term in artwork_search_terms(title) {
        let search_url = format!(
            "https://api.thegamesdb.net/v1/Games/ByGameName?apikey={}&name={}&include=boxart",
            url_encode(api_key),
            url_encode(&search_term),
        );
        let payload: serde_json::Value = ureq::get(&search_url)
            .set("User-Agent", "EmulatorHub/0.0.5-alpha1")
            .call()?
            .into_json()?;
        let base_url = find_original_image_base_url(&payload);
        if let Some(url) = image_url_from_value(&payload, base_url.as_deref()) {
            return Ok(Some(url));
        }

        if let Some(game_id) = first_game_id(&payload) {
            let images_url = format!(
                "https://api.thegamesdb.net/v1/Games/Images?apikey={}&games_id={game_id}",
                url_encode(api_key)
            );
            let images: serde_json::Value = ureq::get(&images_url)
                .set("User-Agent", "EmulatorHub/0.0.5-alpha1")
                .call()?
                .into_json()?;
            let base_url = find_original_image_base_url(&images);
            if let Some(url) = image_url_from_value(&images, base_url.as_deref()) {
                return Ok(Some(url));
            }
        }
    }
    Ok(None)
}

fn download_artwork(url: &str) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let path = cached_artwork_path(url);
    if path.is_file() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = Vec::new();
    ureq::get(url)
        .set("User-Agent", "EmulatorHub/0.0.5-alpha1")
        .call()?
        .into_reader()
        .read_to_end(&mut bytes)?;
    fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(windows)]
fn extract_executable_icon(executable_path: &Path) -> Option<egui::ColorImage> {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits, GetObjectW,
        ReleaseDC,
    };
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    let path: Vec<u16> = executable_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut large_icon: HICON = null_mut();
    let mut small_icon: HICON = null_mut();
    let extracted =
        unsafe { ExtractIconExW(path.as_ptr(), 0, &mut large_icon, &mut small_icon, 1) };
    if extracted == 0 {
        return None;
    }
    let icon = if !large_icon.is_null() {
        large_icon
    } else {
        small_icon
    };
    if icon.is_null() {
        return None;
    }

    let mut icon_info = ICONINFO::default();
    let success = unsafe { GetIconInfo(icon, &mut icon_info) };
    if success == 0 || icon_info.hbmColor.is_null() {
        unsafe {
            if !large_icon.is_null() {
                DestroyIcon(large_icon);
            }
            if !small_icon.is_null() {
                DestroyIcon(small_icon);
            }
        }
        return None;
    }

    let mut bitmap = BITMAP::default();
    let bitmap_size = unsafe {
        GetObjectW(
            icon_info.hbmColor,
            size_of::<BITMAP>() as i32,
            (&mut bitmap as *mut BITMAP).cast(),
        )
    };
    if bitmap_size == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        unsafe {
            DestroyIcon(large_icon);
            if !small_icon.is_null() {
                DestroyIcon(small_icon);
            }
        }
        return None;
    }

    let width = bitmap.bmWidth as usize;
    let height = bitmap.bmHeight as usize;
    let mut pixels = vec![0u8; width.saturating_mul(height).saturating_mul(4)];
    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bitmap.bmWidth,
            biHeight: -(bitmap.bmHeight),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..BITMAPINFOHEADER::default()
        },
        bmiColors: [Default::default()],
    };
    let device_context = unsafe { GetDC(null_mut()) };
    let copied = unsafe {
        GetDIBits(
            device_context,
            icon_info.hbmColor,
            0,
            bitmap.bmHeight as u32,
            pixels.as_mut_ptr().cast(),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };
    if !device_context.is_null() {
        unsafe {
            ReleaseDC(null_mut(), device_context);
        }
    }

    unsafe {
        windows_sys::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmColor);
        if !icon_info.hbmMask.is_null() {
            windows_sys::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmMask);
        }
        DestroyIcon(large_icon);
        if !small_icon.is_null() {
            DestroyIcon(small_icon);
        }
    }
    if copied == 0 {
        return None;
    }

    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if pixel[3] == 0 {
            pixel[3] = 255;
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        &pixels,
    ))
}

#[cfg(not(windows))]
fn extract_executable_icon(_executable_path: &Path) -> Option<egui::ColorImage> {
    None
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
            default_res_height_arg TEXT NOT NULL DEFAULT '-height',
            artwork_api_key TEXT,
            magpie_enabled INTEGER NOT NULL DEFAULT 0,
            magpie_auto_start INTEGER NOT NULL DEFAULT 0,
            magpie_preset TEXT NOT NULL DEFAULT 'FSR',
            magpie_scale REAL NOT NULL DEFAULT 2.0,
            magpie_sharpness REAL NOT NULL DEFAULT 0.87,
            magpie_auto_scale INTEGER NOT NULL DEFAULT 0,
            magpie_hotkey_key TEXT NOT NULL DEFAULT 'A',
            magpie_auto_scale_delay_ms INTEGER NOT NULL DEFAULT 2000
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

fn ensure_app_state_columns(connection: &Connection) -> rusqlite::Result<()> {
    let columns = [
        (
            "artwork_api_key",
            "ALTER TABLE app_state ADD COLUMN artwork_api_key TEXT",
        ),
        (
            "magpie_enabled",
            "ALTER TABLE app_state ADD COLUMN magpie_enabled INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "magpie_auto_start",
            "ALTER TABLE app_state ADD COLUMN magpie_auto_start INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "magpie_preset",
            "ALTER TABLE app_state ADD COLUMN magpie_preset TEXT NOT NULL DEFAULT 'FSR'",
        ),
        (
            "magpie_scale",
            "ALTER TABLE app_state ADD COLUMN magpie_scale REAL NOT NULL DEFAULT 2.0",
        ),
        (
            "magpie_sharpness",
            "ALTER TABLE app_state ADD COLUMN magpie_sharpness REAL NOT NULL DEFAULT 0.87",
        ),
        (
            "magpie_auto_scale",
            "ALTER TABLE app_state ADD COLUMN magpie_auto_scale INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "magpie_hotkey_key",
            "ALTER TABLE app_state ADD COLUMN magpie_hotkey_key TEXT NOT NULL DEFAULT 'A'",
        ),
        (
            "magpie_auto_scale_delay_ms",
            "ALTER TABLE app_state ADD COLUMN magpie_auto_scale_delay_ms INTEGER NOT NULL DEFAULT 2000",
        ),
    ];
    for (name, alter_statement) in columns {
        let column_exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('app_state') WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        if column_exists == 0 {
            connection.execute(alter_statement, [])?;
        }
    }
    Ok(())
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

fn repair_library_metadata(connection: &mut Connection) -> rusqlite::Result<()> {
    let mut rom_statement = connection.prepare(
        "
        SELECT r.id, r.source_path, p.name, r.cover_art_path
        FROM roms r
        JOIN platforms p ON p.id = r.platform_id
        ",
    )?;
    let roms: Vec<(i64, String, String, Option<String>)> = rom_statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(rom_statement);

    let mut emulator_statement = connection.prepare(
        "
        SELECT e.id, e.name, e.executable_path, p.name
        FROM emulators e
        JOIN platforms p ON p.id = e.platform_id
        ",
    )?;
    let emulators: Vec<(i64, String, String, String)> = emulator_statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(emulator_statement);

    let transaction = connection.transaction()?;
    for (id, source_path, platform_name, cover_art_path) in roms {
        let path = Path::new(&source_path);
        let should_repair_platform = is_unknown_platform(&platform_name)
            || (is_ambiguous_rom_platform(path)
                && infer_rom_platform(path) == Some("PlayStation 2"));
        if should_repair_platform {
            if let Some(inferred_platform) = infer_rom_platform(path) {
                let platform_id =
                    get_or_create_platform(&transaction, inferred_platform, now_timestamp())?;
                transaction.execute(
                    "UPDATE roms SET platform_id = ?, updated_at = ? WHERE id = ?",
                    params![platform_id, now_timestamp(), id],
                )?;
            }
        }
        if cover_art_path.is_none() {
            if let Some(cover_art_path) = cover_art_path_for_rom(path) {
                transaction.execute(
                    "UPDATE roms SET cover_art_path = ?, updated_at = ? WHERE id = ?",
                    params![cover_art_path, now_timestamp(), id],
                )?;
            }
        }
    }

    for (id, name, executable_path, platform_name) in emulators {
        if is_unknown_platform(&platform_name) {
            if let Some(inferred_platform) =
                infer_emulator_platform(&name, Path::new(&executable_path))
            {
                let platform_id =
                    get_or_create_platform(&transaction, inferred_platform, now_timestamp())?;
                transaction.execute(
                    "UPDATE emulators SET platform_id = ?, updated_at = ? WHERE id = ?",
                    params![platform_id, now_timestamp(), id],
                )?;
            }
        }
    }

    transaction.commit()
}

fn open_database() -> Result<(Connection, String), Box<dyn Error + Send + Sync>> {
    let path = database_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut connection = Connection::open(path)?;
    init_schema(&connection)?;
    ensure_app_state_columns(&connection)?;
    let migration_status = migrate_legacy_config(&connection)?;
    repair_library_metadata(&mut connection)?;
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
               default_res_height_arg, artwork_api_key,
               magpie_enabled, magpie_auto_start, magpie_preset, magpie_scale,
               magpie_sharpness, magpie_auto_scale, magpie_hotkey_key,
               magpie_auto_scale_delay_ms
        FROM app_state WHERE id = 1
        ",
        [],
        |row| {
            let selected_tab: String = row.get(3)?;
            Ok(StoredState {
                home_dir: row.get::<_, Option<String>>(0)?.map(PathBuf::from),
                current_dir: row.get::<_, Option<String>>(1)?.map(PathBuf::from),
                emulator_dir: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                artwork_api_key: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
                tab: match selected_tab.as_str() {
                    "Upscaling" => Tab::Upscaling,
                    "Settings" => Tab::Settings,
                    _ => Tab::Main,
                },
                magpie_enabled: row.get::<_, i64>(14)? != 0,
                magpie_auto_start: row.get::<_, i64>(15)? != 0,
                magpie_preset: row
                    .get::<_, Option<String>>(16)?
                    .filter(|value| MAGPIE_PRESETS.iter().any(|(name, _)| name == value))
                    .unwrap_or_else(|| "FSR".to_string()),
                magpie_scale: row.get::<_, f64>(17)?.clamp(1.0, 4.0) as f32,
                magpie_sharpness: row.get::<_, f64>(18)?.clamp(0.0, 1.0) as f32,
                magpie_auto_scale: row.get::<_, i64>(19)? != 0,
                magpie_hotkey_key: normalize_magpie_hotkey_key(
                    &row.get::<_, Option<String>>(20)?.unwrap_or_default(),
                ),
                magpie_auto_scale_delay_ms: row.get::<_, i64>(21)?.clamp(500, 10_000) as u64,
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

fn remove_non_emulator_entries(connection: &Connection) -> rusqlite::Result<usize> {
    connection.execute(
        "
        DELETE FROM emulators
        WHERE lower(name) = 'sdl'
           OR lower(name) LIKE 'sdl2%'
           OR lower(name) LIKE '%sdl'
           OR lower(name) LIKE '%sdl2'
           OR lower(name) LIKE '%.sdl'
           OR lower(name) LIKE '%.sdl2'
           OR lower(name) LIKE 'unins%'
           OR lower(name) LIKE 'uninstall%'
        ",
        [],
    )
}

fn load_roms(connection: &Connection) -> rusqlite::Result<Vec<Rom>> {
    let mut statement = connection.prepare(
        "
        SELECT r.id, r.title, r.source_path, r.platform_id, p.name,
               r.cover_art_path, r.favorite, r.last_played_at, r.play_count
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
            cover_art_path: row.get(5)?,
            favorite: row.get::<_, i64>(6)? != 0,
            last_played_at: row.get(7)?,
            play_count: row.get(8)?,
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

fn version_tuple(version: &str) -> Option<(u64, u64, u64, u64, bool)> {
    let version = version.trim().trim_start_matches('v');
    let mut version_parts = version.splitn(2, '-');
    let version = version_parts.next()?;
    let is_stable = version_parts.next().is_none();
    let mut parts = version.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next().unwrap_or("0").parse().ok()?,
        is_stable,
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

fn bundled_magpie_path() -> Option<PathBuf> {
    let installed_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|directory| directory.join("tools").join("Magpie").join("Magpie.exe"));
    if installed_path.as_ref().is_some_and(|path| path.is_file()) {
        return installed_path;
    }

    let development_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("installer")
        .join("Magpie")
        .join("Magpie.exe");
    development_path.is_file().then_some(development_path)
}

fn normalize_magpie_hotkey_key(value: &str) -> String {
    let key = value.trim().to_ascii_uppercase();
    if key.len() == 1 && key.as_bytes()[0].is_ascii_alphabetic() {
        key
    } else {
        "A".to_string()
    }
}

#[cfg(windows)]
fn magpie_hotkey_virtual_key(value: &str) -> u8 {
    normalize_magpie_hotkey_key(value).as_bytes()[0]
}

#[cfg(windows)]
struct MagpieWindowSearch {
    process_id: u32,
    window: windows_sys::Win32::Foundation::HWND,
}

#[cfg(windows)]
unsafe extern "system" fn find_magpie_target_window(
    window: windows_sys::Win32::Foundation::HWND,
    parameter: windows_sys::Win32::Foundation::LPARAM,
) -> i32 {
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe {
        let search = &mut *(parameter as *mut MagpieWindowSearch);
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id == search.process_id
            && IsWindowVisible(window) != 0
            && GetWindow(window, GW_OWNER) == null_mut()
        {
            search.window = window;
            return 0;
        }
    }
    1
}

#[cfg(windows)]
fn find_window_for_process(process_id: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;

    let mut search = MagpieWindowSearch {
        process_id,
        window: null_mut(),
    };
    unsafe {
        EnumWindows(
            Some(find_magpie_target_window),
            &mut search as *mut MagpieWindowSearch as isize,
        );
    }
    (!search.window.is_null()).then_some(search.window)
}

#[cfg(windows)]
fn send_magpie_scale_hotkey(process_id: u32, hotkey_key: &str) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        KEYEVENTF_KEYUP, VK_MENU, VK_SHIFT, keybd_event,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    let Some(window) = find_window_for_process(process_id) else {
        return false;
    };
    unsafe {
        ShowWindow(window, SW_RESTORE);
        SetForegroundWindow(window);
        thread::sleep(Duration::from_millis(75));
        keybd_event(VK_MENU as u8, 0, 0, 0);
        keybd_event(VK_SHIFT as u8, 0, 0, 0);
        keybd_event(magpie_hotkey_virtual_key(hotkey_key), 0, 0, 0);
        keybd_event(magpie_hotkey_virtual_key(hotkey_key), 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_SHIFT as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
    }
    true
}

fn magpie_scaling_modes_path() -> Option<PathBuf> {
    bundled_magpie_path().and_then(|path| {
        path.parent()
            .map(|directory| directory.join("ScalingModes.json"))
    })
}

fn magpie_effect(name: &str, scale: Option<f32>) -> serde_json::Value {
    let mut effect = serde_json::json!({ "name": name });
    if let Some(scale) = scale {
        effect["scalingType"] = serde_json::json!(1);
        effect["scale"] = serde_json::json!({ "x": scale, "y": scale });
    }
    effect
}

fn magpie_profile_json(preset: &str, scale: f32, sharpness: f32) -> serde_json::Value {
    let effects = match preset {
        "Anime4K" => vec![magpie_effect("Anime4K\\Anime4K_Upscale_L", None)],
        "CAS" => {
            let mut effect = magpie_effect("CAS\\CAS_Scaling", Some(scale));
            effect["sharpness"] = serde_json::json!(sharpness);
            vec![effect]
        }
        "xBRZ" => vec![magpie_effect("xBRZ\\xBRZ_2x", None)],
        "Lanczos" => vec![magpie_effect("Lanczos", Some(scale))],
        "Nearest" => vec![magpie_effect("Nearest", Some(scale))],
        _ => {
            let mut rcas = magpie_effect("FSR\\FSR_RCAS", None);
            rcas["sharpness"] = serde_json::json!(sharpness);
            vec![magpie_effect("FSR\\FSR_EASU", Some(scale)), rcas]
        }
    };
    serde_json::json!({
        "name": format!("Emulator Hub - {preset}"),
        "effects": effects,
    })
}

impl App {
    fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let (db, migration_status) = open_database()?;
        // Older builds could import helper executables such as SDL launchers
        // and uninstaller programs when scanning an emulator folder. Remove
        // those library entries once at startup so only actual emulators show.
        remove_non_emulator_entries(&db)?;
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
            search_query: String::new(),
            library_filter: LibraryFilter::All,
            selected_rom_id: None,
            cover_art_textures: HashMap::new(),
            emulator_icon_textures: HashMap::new(),
            artwork_api_key: stored_state.artwork_api_key,
            magpie_enabled: stored_state.magpie_enabled,
            magpie_auto_start: stored_state.magpie_auto_start,
            magpie_preset: stored_state.magpie_preset,
            magpie_scale: stored_state.magpie_scale,
            magpie_sharpness: stored_state.magpie_sharpness,
            magpie_auto_scale: stored_state.magpie_auto_scale,
            magpie_hotkey_key: stored_state.magpie_hotkey_key,
            magpie_auto_scale_delay_ms: stored_state.magpie_auto_scale_delay_ms,
            magpie_process: None,
            pending_auto_scale: None,
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
            Tab::Upscaling => "Upscaling",
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
                default_res_height_arg = ?13,
                artwork_api_key = ?14,
                magpie_enabled = ?15,
                magpie_auto_start = ?16,
                magpie_preset = ?17,
                magpie_scale = ?18,
                magpie_sharpness = ?19,
                magpie_auto_scale = ?20,
                magpie_hotkey_key = ?21,
                magpie_auto_scale_delay_ms = ?22
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
                self.artwork_api_key.trim(),
                self.magpie_enabled as i64,
                self.magpie_auto_start as i64,
                self.magpie_preset,
                self.magpie_scale as f64,
                self.magpie_sharpness as f64,
                self.magpie_auto_scale as i64,
                normalize_magpie_hotkey_key(&self.magpie_hotkey_key),
                self.magpie_auto_scale_delay_ms.clamp(500, 10_000) as i64,
            ],
        )?;
        Ok(())
    }

    fn magpie_is_running(&mut self) -> bool {
        let Some(process) = &mut self.magpie_process else {
            return false;
        };
        match process.try_wait() {
            Ok(Some(_)) => {
                self.magpie_process = None;
                false
            }
            Ok(None) => true,
            Err(_) => {
                self.magpie_process = None;
                false
            }
        }
    }

    fn start_magpie(&mut self) {
        if self.magpie_is_running() {
            self.status = "Magpie is already running.".to_string();
            return;
        }

        let Some(executable) = bundled_magpie_path() else {
            self.status =
                "Bundled Magpie was not found. Reinstall Emulator Hub or use the source checkout."
                    .to_string();
            return;
        };
        let Some(directory) = executable.parent() else {
            self.status = "Could not locate Magpie's application folder.".to_string();
            return;
        };

        match Command::new(&executable).current_dir(directory).spawn() {
            Ok(process) => {
                self.magpie_process = Some(process);
                self.magpie_enabled = true;
                let _ = self.save_app_state();
                self.status = "Magpie started in the background.".to_string();
            }
            Err(error) => self.status = format!("Could not start Magpie: {error}"),
        }
    }

    fn stop_magpie(&mut self) {
        let Some(mut process) = self.magpie_process.take() else {
            return;
        };
        let _ = process.kill();
        let _ = process.wait();
        self.status = "Magpie stopped.".to_string();
    }

    fn process_pending_auto_scale(&mut self) {
        #[cfg(windows)]
        {
            let Some(pending) = self.pending_auto_scale.clone() else {
                return;
            };
            let now = Instant::now();
            if now < pending.not_before {
                return;
            }
            if send_magpie_scale_hotkey(pending.process_id, &self.magpie_hotkey_key) {
                self.pending_auto_scale = None;
                self.status = format!(
                    "Automatically applied Magpie scaling with Alt+Shift+{}.",
                    self.magpie_hotkey_key
                );
            } else if now >= pending.expires_at {
                self.pending_auto_scale = None;
                self.status =
                    "ROM started, but its window was not found for automatic Magpie scaling."
                        .to_string();
            }
        }

        #[cfg(not(windows))]
        {
            self.pending_auto_scale = None;
        }
    }

    fn apply_magpie_profile(&mut self) {
        let Some(path) = magpie_scaling_modes_path() else {
            self.status = "Bundled Magpie was not found.".to_string();
            return;
        };

        let was_running = self.magpie_is_running();
        if was_running {
            self.stop_magpie();
        }

        let result = (|| -> Result<(), Box<dyn Error + Send + Sync>> {
            let mut root = if path.is_file() {
                serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)?
            } else {
                serde_json::json!({})
            };
            let object = root
                .as_object_mut()
                .ok_or("Magpie's ScalingModes.json must contain an object.")?;
            let modes = object
                .entry("scalingModes")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or("Magpie's scalingModes value must be an array.")?;
            let profile = magpie_profile_json(
                &self.magpie_preset,
                self.magpie_scale,
                self.magpie_sharpness,
            );
            let profile_name = profile["name"].as_str().unwrap_or_default();
            if let Some(existing) = modes
                .iter_mut()
                .find(|mode| mode["name"].as_str() == Some(profile_name))
            {
                *existing = profile;
            } else {
                modes.push(profile);
            }
            fs::write(&path, serde_json::to_string_pretty(&root)?)?;
            Ok(())
        })();

        if was_running {
            self.start_magpie();
        }
        match result {
            Ok(()) => {
                let _ = self.save_app_state();
                self.status = format!(
                    "Applied the {} Magpie profile. Select 'Emulator Hub - {}' in Magpie.",
                    self.magpie_preset, self.magpie_preset
                );
            }
            Err(error) => self.status = format!("Could not apply Magpie profile: {error}"),
        }
    }

    fn import_rom_paths(&mut self, paths: Vec<PathBuf>) {
        let requested_platform = platform_name_or_unknown(&self.platform_input);
        let timestamp = now_timestamp();
        let result = (|| -> rusqlite::Result<usize> {
            let transaction = self.db.transaction()?;
            let mut imported = 0;
            for path in paths {
                if !path.is_file() || is_executable(&path) {
                    continue;
                }
                let platform_name = if is_unknown_platform(&requested_platform) {
                    infer_rom_platform(&path).unwrap_or("Unknown")
                } else {
                    requested_platform.as_str()
                };
                let platform_id = get_or_create_platform(&transaction, platform_name, timestamp)?;
                let (file_size, modified_at) = file_metadata(&path);
                transaction.execute(
                    "
                    INSERT INTO roms (
                        title, source_path, source_path_key, platform_id,
                        cover_art_path, file_size, modified_at, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                    ON CONFLICT(source_path_key) DO UPDATE SET
                        title = excluded.title,
                        source_path = excluded.source_path,
                        platform_id = excluded.platform_id,
                        cover_art_path = COALESCE(excluded.cover_art_path, roms.cover_art_path),
                        file_size = excluded.file_size,
                        modified_at = excluded.modified_at,
                        updated_at = excluded.updated_at
                    ",
                    params![
                        title_from_path(&path),
                        path_string(&path),
                        normalized_path_key(&path),
                        platform_id,
                        cover_art_path_for_rom(&path),
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
                self.auto_fetch_missing_artwork();
            }
            Err(error) => self.status = format!("ROM import failed: {error}"),
        }
    }

    fn import_emulator_paths(&mut self, paths: Vec<PathBuf>) {
        let requested_platform = platform_name_or_unknown(&self.platform_input);
        let timestamp = now_timestamp();
        let defaults = self.default_settings.clone();
        let result = (|| -> rusqlite::Result<usize> {
            let transaction = self.db.transaction()?;
            let mut imported = 0;
            for path in paths {
                if !path.is_file() || !is_executable(&path) || is_non_emulator_executable(&path) {
                    continue;
                }
                let platform_name = if is_unknown_platform(&requested_platform) {
                    infer_emulator_platform(&title_from_path(&path), &path).unwrap_or("Unknown")
                } else {
                    requested_platform.as_str()
                };
                let platform_id = get_or_create_platform(&transaction, platform_name, timestamp)?;
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

        if self.magpie_enabled && (self.magpie_auto_start || self.magpie_auto_scale) {
            self.start_magpie();
        }

        let mut command = Command::new(&emulator.executable_path);
        // Keep options before the ROM path for conventional command-line
        // parsers that expect "[options] file".
        append_launch_arguments(&mut command, &emulator.settings, &emulator.executable_path);
        command.arg(&rom.source_path);
        match command.spawn() {
            Ok(child) => {
                if self.magpie_enabled && self.magpie_auto_scale {
                    let now = Instant::now();
                    self.pending_auto_scale = Some(PendingAutoScale {
                        process_id: child.id(),
                        not_before: now + Duration::from_millis(self.magpie_auto_scale_delay_ms),
                        expires_at: now + Duration::from_secs(15),
                    });
                }
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
                self.status = "Favorite status updated.".to_string();
            }
            Err(error) => self.status = format!("Could not update favorite: {error}"),
        }
    }

    fn load_cover_art_texture(
        &mut self,
        context: &egui::Context,
        cover_art_path: &str,
    ) -> Option<TextureHandle> {
        let cache_key = normalized_path_key(Path::new(cover_art_path));
        if let Some(texture) = self.cover_art_textures.get(&cache_key) {
            return Some(texture.clone());
        }

        let decoded = ImageReader::open(cover_art_path).ok()?.decode().ok()?;
        let image = decoded.thumbnail(512, 512).to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = context.load_texture(
            format!("rom-cover-{cache_key}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.cover_art_textures.insert(cache_key, texture.clone());
        Some(texture)
    }

    fn show_rom_visual(&mut self, ui: &mut Ui, rom: &Rom, missing: bool) {
        if !missing {
            if let Some(cover_art_path) = rom.cover_art_path.as_deref() {
                if let Some(texture) = self.load_cover_art_texture(ui.ctx(), cover_art_path) {
                    ui.add(
                        egui::Image::from_texture(&texture).fit_to_exact_size(Vec2::splat(64.0)),
                    );
                    return;
                }
            }
        }

        let platform_icon = if missing {
            "!".to_string()
        } else {
            icon_label(&rom.platform_name)
        };
        show_icon_tile(
            ui,
            &platform_icon,
            accent_for_name(&rom.platform_name),
            missing,
        );
    }

    fn load_emulator_icon_texture(
        &mut self,
        context: &egui::Context,
        executable_path: &str,
    ) -> Option<TextureHandle> {
        let cache_key = normalized_path_key(Path::new(executable_path));
        if let Some(texture) = self.emulator_icon_textures.get(&cache_key) {
            return Some(texture.clone());
        }
        let image = extract_executable_icon(Path::new(executable_path))?;
        let texture = context.load_texture(
            format!("emulator-icon-{cache_key}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.emulator_icon_textures
            .insert(cache_key, texture.clone());
        Some(texture)
    }

    fn show_emulator_visual(&mut self, ui: &mut Ui, emulator: &Emulator, missing: bool) {
        if !missing {
            if let Some(texture) =
                self.load_emulator_icon_texture(ui.ctx(), &emulator.executable_path)
            {
                ui.add(egui::Image::from_texture(&texture).fit_to_exact_size(Vec2::splat(46.0)));
                return;
            }
        }

        let emulator_icon = if missing {
            "!".to_string()
        } else {
            icon_label(&emulator.name)
        };
        show_icon_tile(ui, &emulator_icon, accent_for_name(&emulator.name), missing);
    }

    fn set_cover_art(&mut self, rom_id: i64, cover_art_path: Option<PathBuf>) {
        let path_text = cover_art_path.as_ref().map(|path| path_string(path));
        match self.db.execute(
            "UPDATE roms SET cover_art_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![path_text, now_timestamp(), rom_id],
        ) {
            Ok(_) => {
                if let Some(path) = cover_art_path {
                    self.cover_art_textures.remove(&normalized_path_key(&path));
                    self.status = "ROM artwork updated.".to_string();
                } else {
                    self.status = "ROM artwork cleared.".to_string();
                }
                let _ = self.refresh_from_db();
            }
            Err(error) => self.status = format!("Could not update ROM artwork: {error}"),
        }
    }

    fn fetch_artwork_for_rom(&mut self, rom_id: i64) {
        let Some(rom) = self.roms.iter().find(|rom| rom.id == rom_id).cloned() else {
            return;
        };
        let api_key = self.artwork_api_key.trim().to_string();
        if api_key.is_empty() {
            self.status =
                "Add your TheGamesDB API key in Settings before fetching artwork.".to_string();
            return;
        }

        self.status = format!("Finding artwork for '{}'...", rom.title);
        match fetch_artwork_url(&api_key, &rom.title, &rom.platform_name) {
            Ok(Some(url)) => match download_artwork(&url) {
                Ok(path) => {
                    let path_text = path_string(&path);
                    match self.db.execute(
                        "UPDATE roms SET cover_art_path = ?1, updated_at = ?2 WHERE id = ?3",
                        params![path_text, now_timestamp(), rom.id],
                    ) {
                        Ok(_) => {
                            self.cover_art_textures.remove(&normalized_path_key(&path));
                            let _ = self.refresh_from_db();
                            self.status = format!("Artwork downloaded for '{}'.", rom.title);
                        }
                        Err(error) => {
                            self.status = format!("Could not save artwork path: {error}");
                        }
                    }
                }
                Err(error) => self.status = format!("Could not download artwork: {error}"),
            },
            Ok(None) => {
                self.status = format!("No artwork found for '{}'.", rom.title);
            }
            Err(error) => self.status = format!("Artwork lookup failed: {error}"),
        }
    }

    fn auto_fetch_missing_artwork(&mut self) {
        if self.artwork_api_key.trim().is_empty() {
            return;
        }
        let ids: Vec<i64> = self
            .roms
            .iter()
            .filter(|rom| rom.cover_art_path.is_none())
            .map(|rom| rom.id)
            .take(10)
            .collect();
        for rom_id in ids {
            self.fetch_artwork_for_rom(rom_id);
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

    fn open_emulator_settings(&mut self, emulator_id: i64) {
        let Some(emulator) = self
            .emulators
            .iter()
            .find(|emulator| emulator.id == emulator_id)
            .cloned()
        else {
            return;
        };

        // Configure can be opened directly from the home screen. Explicitly
        // load the selected emulator's profile here instead of waiting for the
        // settings ComboBox to change, otherwise the page can still show the
        // default profile while the emulator is selected.
        self.selected_emulator_id = Some(emulator.id);
        self.editing_settings = emulator.settings;
        self.tab = Tab::Settings;
        self.status = format!("Editing settings for '{}'.", emulator.name);
    }

    fn show_upscaling_tab(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Upscaling").size(22.0).strong());
        ui.label(
            RichText::new("Use the bundled Magpie companion to scale emulator windows.")
                .color(TEXT_MUTED),
        );
        ui.add_space(12.0);

        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
            let executable = bundled_magpie_path();
            let running = self.magpie_is_running();
            ui.horizontal(|ui| {
                ui.label(RichText::new("MAGPIE").small().color(TEXT_MUTED).strong());
                ui.label(
                    RichText::new(if running { "Running" } else { "Stopped" }).color(if running {
                        ACCENT_GREEN
                    } else {
                        TEXT_MUTED
                    }),
                );
            });
            ui.label(
                RichText::new(
                    executable
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "Bundled Magpie executable not found".to_string()),
                )
                .small()
                .color(if executable.is_some() {
                    TEXT_MUTED
                } else {
                    DANGER
                }),
            );
            ui.add_space(8.0);

            let mut settings_changed = false;
            settings_changed |= ui
                .checkbox(&mut self.magpie_enabled, "Enable Magpie integration")
                .changed();
            settings_changed |= ui
                .add_enabled(
                    self.magpie_enabled,
                    egui::Checkbox::new(
                        &mut self.magpie_auto_start,
                        "Start Magpie automatically when launching a ROM",
                    ),
                )
                .changed();
            settings_changed |= ui
                .add_enabled(
                    self.magpie_enabled,
                    egui::Checkbox::new(
                        &mut self.magpie_auto_scale,
                        "Automatically apply scaling when a ROM window opens",
                    ),
                )
                .changed();
            ui.horizontal(|ui| {
                ui.label("Auto-scale hotkey: Alt+Shift+");
                settings_changed |= ui
                    .add_enabled(
                        self.magpie_enabled && self.magpie_auto_scale,
                        TextEdit::singleline(&mut self.magpie_hotkey_key)
                            .desired_width(40.0)
                            .char_limit(1),
                    )
                    .changed();
                ui.label("Delay:");
                settings_changed |= ui
                    .add_enabled(
                        self.magpie_enabled && self.magpie_auto_scale,
                        egui::Slider::new(&mut self.magpie_auto_scale_delay_ms, 500..=10_000)
                            .suffix(" ms"),
                    )
                    .changed();
            });
            ui.label(
                RichText::new(
                    "Set the same Alt+Shift+key in Magpie once. Emulator Hub will focus the \
                     emulator window and send that shortcut automatically after launch.",
                )
                .small()
                .color(TEXT_MUTED),
            );

            ui.separator();
            ui.label(RichText::new("Emulator Hub profile").strong());
            let previous_preset = self.magpie_preset.clone();
            ComboBox::from_id_salt(Id::new("magpie_preset"))
                .selected_text(
                    MAGPIE_PRESETS
                        .iter()
                        .find(|(name, _)| *name == self.magpie_preset)
                        .map(|(name, description)| format!("{name} — {description}"))
                        .unwrap_or_else(|| self.magpie_preset.clone()),
                )
                .show_ui(ui, |ui| {
                    for (name, description) in MAGPIE_PRESETS {
                        ui.selectable_value(
                            &mut self.magpie_preset,
                            (*name).to_string(),
                            format!("{name} — {description}"),
                        );
                    }
                });
            let scale_changed = ui
                .add(
                    egui::Slider::new(&mut self.magpie_scale, 1.0..=4.0)
                        .text("Scale multiplier")
                        .clamping(egui::SliderClamping::Always),
                )
                .changed();
            let sharpness_changed = ui
                .add(
                    egui::Slider::new(&mut self.magpie_sharpness, 0.0..=1.0)
                        .text("Sharpening")
                        .clamping(egui::SliderClamping::Always),
                )
                .changed();
            if previous_preset != self.magpie_preset || scale_changed || sharpness_changed {
                settings_changed = true;
            }
            ui.label(
                RichText::new(
                    "Apply Profile writes a reusable Magpie scaling mode. Effects such as \
                     Anime4K and xBRZ use their own fixed scaling behavior.",
                )
                .small()
                .color(TEXT_MUTED),
            );

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(executable.is_some(), egui::Button::new("Start Magpie"))
                    .clicked()
                {
                    self.start_magpie();
                }
                if ui
                    .add_enabled(running, egui::Button::new("Stop Magpie"))
                    .clicked()
                {
                    self.stop_magpie();
                }
                if ui
                    .add_enabled(executable.is_some(), egui::Button::new("Apply Profile"))
                    .clicked()
                {
                    self.apply_magpie_profile();
                }
            });

            if !self.magpie_enabled && running {
                self.stop_magpie();
            }
            if settings_changed {
                if let Err(error) = self.save_app_state() {
                    self.status = format!("Could not save upscaling settings: {error}");
                } else if self.magpie_enabled {
                    self.status = "Upscaling settings saved.".to_string();
                }
            }
        });

        ui.add_space(12.0);
        card_frame(PANEL_RAISED, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
            ui.label(RichText::new("How to use it").strong());
            ui.add_space(4.0);
            ui.label(
                "1. Start a ROM in windowed mode.\n\
                 2. Choose a profile and scale multiplier, then click Apply Profile.\n\
                 3. Start Magpie here, or enable automatic startup.\n\
                 4. Select the emulator window and use Magpie's configured scaling hotkey.\n\
                 5. Configure advanced filters and hotkeys inside Magpie when needed.",
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Magpie remains a separate background process because it captures and scales \
                     the emulator window. Emulator Hub starts and stops the bundled copy for you.",
                )
                .small()
                .color(TEXT_MUTED),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Bundled Magpie is provided under its own open-source license. See the \
                     installed tools\\Magpie\\LICENSE.txt file.",
                )
                .small()
                .color(TEXT_MUTED),
            );
        });
    }

    fn modern_visible_roms(&self) -> Vec<Rom> {
        let query = self.search_query.trim().to_lowercase();
        self.roms
            .iter()
            .filter(|rom| {
                let matches_filter = match self.library_filter {
                    LibraryFilter::All => true,
                    LibraryFilter::Favorites => rom.favorite,
                };
                let matches_search = query.is_empty()
                    || rom.title.to_lowercase().contains(&query)
                    || rom.platform_name.to_lowercase().contains(&query);
                matches_filter && matches_search
            })
            .cloned()
            .collect()
    }

    fn show_modern_sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(18.0);
        ui.label(
            RichText::new("EMULATOR HUB")
                .strong()
                .color(ACCENT)
                .size(16.0),
        );
        ui.label(RichText::new("Your game library").small().color(TEXT_MUTED));
        ui.add_space(24.0);
        ui.label(RichText::new("LIBRARY").small().color(TEXT_MUTED).strong());
        ui.add_space(6.0);

        if ui
            .selectable_label(
                self.tab == Tab::Main && self.library_filter == LibraryFilter::All,
                RichText::new("▦  Library").size(14.0),
            )
            .clicked()
        {
            self.tab = Tab::Main;
            self.library_filter = LibraryFilter::All;
        }
        if ui
            .selectable_label(
                self.tab == Tab::Main && self.library_filter == LibraryFilter::Favorites,
                RichText::new("★  Favorites").size(14.0),
            )
            .clicked()
        {
            self.tab = Tab::Main;
            self.library_filter = LibraryFilter::Favorites;
        }

        ui.add_space(20.0);
        ui.label(RichText::new("TOOLS").small().color(TEXT_MUTED).strong());
        ui.add_space(6.0);
        if ui
            .selectable_label(
                self.tab == Tab::Upscaling,
                RichText::new("↗  Upscaling").size(14.0),
            )
            .clicked()
        {
            self.tab = Tab::Upscaling;
        }
        if ui
            .selectable_label(
                self.tab == Tab::Settings,
                RichText::new("⚙  Settings").size(14.0),
            )
            .clicked()
        {
            self.tab = Tab::Settings;
        }

        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.separator();
            ui.label(
                RichText::new(format!("Version {APP_VERSION}"))
                    .small()
                    .color(TEXT_MUTED),
            );
            ui.label(
                RichText::new("SQLite library connected")
                    .small()
                    .color(ACCENT_GREEN),
            );
        });
    }

    fn show_modern_top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match self.tab {
                    Tab::Main => "Library",
                    Tab::Upscaling => "Upscaling",
                    Tab::Settings => "Settings",
                })
                .strong()
                .size(20.0),
            );
            ui.label(
                RichText::new(match self.tab {
                    Tab::Main => "Manage your emulators and ROM collection",
                    Tab::Upscaling => "Scale emulator windows with bundled Magpie",
                    Tab::Settings => "Configure launch behavior and updates",
                })
                .small()
                .color(TEXT_MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_sized(
                    [220.0, 30.0],
                    TextEdit::singleline(&mut self.search_query).hint_text("Search library..."),
                );
                ui.label(RichText::new("⌕").size(20.0).color(TEXT_MUTED));
            });
        });
    }

    fn show_modern_rom_card(&mut self, ui: &mut Ui, rom: Rom) {
        let missing = !Path::new(&rom.source_path).exists();
        let selected = self.selected_rom_id == Some(rom.id);
        let border = if missing {
            DANGER
        } else if selected {
            ACCENT
        } else {
            Color32::from_rgb(43, 75, 88)
        };

        card_frame(PANEL_BACKGROUND, border).show(ui, |ui| {
            ui.horizontal(|ui| {
                self.show_rom_visual(ui, &rom, missing);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&rom.title).size(16.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "{}  -  {} plays",
                            rom.platform_name, rom.play_count
                        ))
                        .small()
                        .color(TEXT_MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    if ui
                        .button(if rom.favorite {
                            "\u{2605} Favorite"
                        } else {
                            "\u{2606} Favorite"
                        })
                        .on_hover_text("Toggle favorite")
                        .clicked()
                    {
                        self.toggle_favorite(rom.id);
                    }
                });
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(if missing {
                    "ROM file is missing"
                } else {
                    "Ready to launch"
                })
                .small()
                .color(if missing { DANGER } else { ACCENT_GREEN }),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    self.selected_rom_id = Some(rom.id);
                    self.launch_rom(rom.id);
                }
                if ui.button("Details").clicked() {
                    self.selected_rom_id = Some(rom.id);
                }
                ui.label(
                    RichText::new(if rom.last_played_at.is_some() {
                        "Played before"
                    } else {
                        "Never played"
                    })
                    .small()
                    .color(TEXT_MUTED),
                );
            });
        });
    }

    fn show_modern_details(&mut self, ui: &mut Ui) {
        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
            ui.label(
                RichText::new("SELECTED ROM")
                    .small()
                    .color(TEXT_MUTED)
                    .strong(),
            );
            ui.add_space(8.0);
            let selected_rom = self
                .selected_rom_id
                .and_then(|id| self.roms.iter().find(|rom| rom.id == id))
                .cloned();
            if let Some(rom) = selected_rom {
                let missing = !Path::new(&rom.source_path).exists();
                ui.horizontal(|ui| {
                    self.show_rom_visual(ui, &rom, missing);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&rom.title).size(20.0).strong());
                        ui.label(RichText::new(&rom.platform_name).color(ACCENT));
                    });
                });
                ui.separator();
                ui.label(
                    RichText::new("SOURCE FILE")
                        .small()
                        .color(TEXT_MUTED)
                        .strong(),
                );
                ui.label(RichText::new(&rom.source_path).small().color(TEXT_MUTED));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Played {}", rom.play_count)));
                    if rom.favorite {
                        ui.label(RichText::new("\u{2605} Favorite").color(ACCENT_GREEN));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Fetch artwork").clicked() {
                        self.fetch_artwork_for_rom(rom.id);
                    }
                    if ui.button("Choose artwork").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Artwork", &["png", "jpg", "jpeg"])
                            .pick_file()
                        {
                            self.set_cover_art(rom.id, Some(path));
                        }
                    }
                    if rom.cover_art_path.is_some() && ui.button("Clear artwork").clicked() {
                        self.set_cover_art(rom.id, None);
                    }
                });
                ui.add_space(8.0);
                if missing {
                    ui.label(
                        RichText::new("This file is no longer at its imported location.")
                            .small()
                            .color(DANGER),
                    );
                } else if ui.button("Launch ROM").clicked() {
                    self.launch_rom(rom.id);
                }
            } else {
                ui.label(RichText::new("Choose a ROM card to see its details.").color(TEXT_MUTED));
            }
        });
    }

    fn show_modern_emulators(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("EMULATORS")
                        .small()
                        .color(TEXT_MUTED)
                        .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(self.emulators.len().to_string()).color(ACCENT));
                });
            });
            ui.add_space(8.0);
            if self.emulators.is_empty() {
                ui.label(RichText::new("No emulators imported yet.").color(TEXT_MUTED));
            } else {
                for emulator in self.emulators.clone().into_iter().take(6) {
                    let missing = !Path::new(&emulator.executable_path).exists();
                    ui.horizontal(|ui| {
                        self.show_emulator_visual(ui, &emulator, missing);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&emulator.name).strong());
                            ui.label(
                                RichText::new(&emulator.platform_name)
                                    .small()
                                    .color(TEXT_MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Configure").clicked() {
                                self.open_emulator_settings(emulator.id);
                            }
                        });
                    });
                    ui.add_space(5.0);
                }
            }
        });
    }

    fn show_modern_main(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your collection").size(22.0).strong());
            ui.label(
                RichText::new(match self.library_filter {
                    LibraryFilter::All => "All imported ROMs",
                    LibraryFilter::Favorites => "Favorite ROMs",
                })
                .color(TEXT_MUTED),
            );
        });
        ui.add_space(10.0);

        let visible_roms = self.modern_visible_roms();
        ui.columns(2, |columns| {
            columns[0].vertical(|ui| {
                card_frame(PANEL_RAISED, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("PLATFORM").small().color(TEXT_MUTED).strong());
                        ui.add_sized(
                            [190.0, 30.0],
                            TextEdit::singleline(&mut self.platform_input)
                                .hint_text("NES, SNES, PlayStation"),
                        );
                        ui.separator();
                        if ui.button("+  Import ROMs").clicked() {
                            if let Some(paths) = FileDialog::new().pick_files() {
                                self.import_rom_paths(paths);
                            }
                        }
                        if ui.button("+  ROM folder").clicked() {
                            self.import_rom_folder();
                        }
                        if ui.button("+  Emulator").clicked() {
                            if let Some(path) = FileDialog::new().pick_file() {
                                self.import_emulator_paths(vec![path]);
                            }
                        }
                        if ui.button("+  Emulator folder").clicked() {
                            self.import_emulator_folder();
                        }
                    });
                });

                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, value, color) in [
                        ("ROMs", self.roms.len(), ACCENT),
                        (
                            "Favorites",
                            self.roms.iter().filter(|rom| rom.favorite).count(),
                            ACCENT_GREEN,
                        ),
                        (
                            "Emulators",
                            self.emulators.len(),
                            Color32::from_rgb(191, 157, 255),
                        ),
                        (
                            "Platforms",
                            self.platforms.len(),
                            Color32::from_rgb(255, 188, 105),
                        ),
                    ] {
                        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(
                            ui,
                            |ui| {
                                ui.label(RichText::new(label).small().color(TEXT_MUTED));
                                ui.label(
                                    RichText::new(value.to_string())
                                        .size(24.0)
                                        .strong()
                                        .color(color),
                                );
                            },
                        );
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ROM LIBRARY")
                            .small()
                            .color(TEXT_MUTED)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(visible_roms.len().to_string()).color(ACCENT));
                    });
                });
                ui.add_space(8.0);
                ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    if visible_roms.is_empty() {
                        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(
                            ui,
                            |ui| {
                                ui.label(
                                    RichText::new(if self.roms.is_empty() {
                                        "Import your first ROM to start building the library."
                                    } else {
                                        "No ROMs match the current search or filter."
                                    })
                                    .color(TEXT_MUTED),
                                );
                            },
                        );
                    } else {
                        ui.columns(2, |cards| {
                            for (index, rom) in visible_roms.iter().cloned().enumerate() {
                                Self::show_modern_rom_card(self, &mut cards[index % 2], rom);
                                cards[index % 2].add_space(10.0);
                            }
                        });
                    }
                });
            });
            columns[1].vertical(|ui| {
                self.show_modern_details(ui);
                self.show_modern_emulators(ui);
            });
        });

        // Keep navigation controls at the bottom of the available content area
        // instead of directly under the library cards.
        let footer_height = 34.0;
        let remaining_height = ui.available_height() - footer_height;
        if remaining_height > 0.0 {
            ui.add_space(remaining_height);
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Home").clicked() {
                if let Some(home) = &self.home_dir {
                    self.current_dir = home.clone();
                    let _ = self.save_app_state();
                }
            }
            if ui.button("Go up").clicked() && self.current_dir.pop() {
                let _ = self.save_app_state();
            }
            if ui.button("Refresh library").clicked() {
                let _ = self.refresh_from_db();
            }
            ui.label(RichText::new(&self.status).small().color(TEXT_MUTED));
            if !self.current_dir.as_os_str().is_empty() {
                ui.label(
                    RichText::new(format!("Current directory: {}", self.current_dir.display()))
                        .small()
                        .color(TEXT_MUTED),
                );
            }
        });
    }

    #[allow(dead_code)]
    fn show_modern_main_legacy(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your collection").size(22.0).strong());
            ui.label(
                RichText::new(match self.library_filter {
                    LibraryFilter::All => "All imported ROMs",
                    LibraryFilter::Favorites => "Favorite ROMs",
                })
                .color(TEXT_MUTED),
            );
        });
        ui.add_space(10.0);

        card_frame(PANEL_RAISED, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("PLATFORM").small().color(TEXT_MUTED).strong());
                ui.add_sized(
                    [190.0, 30.0],
                    TextEdit::singleline(&mut self.platform_input)
                        .hint_text("NES, SNES, PlayStation"),
                );
                ui.separator();
                if ui.button("＋  Import ROMs").clicked() {
                    if let Some(paths) = FileDialog::new().pick_files() {
                        self.import_rom_paths(paths);
                    }
                }
                if ui.button("＋  ROM folder").clicked() {
                    self.import_rom_folder();
                }
                if ui.button("＋  Emulator").clicked() {
                    if let Some(path) = FileDialog::new().pick_file() {
                        self.import_emulator_paths(vec![path]);
                    }
                }
                if ui.button("＋  Emulator folder").clicked() {
                    self.import_emulator_folder();
                }
            });
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            for (label, value, color) in [
                ("ROMs", self.roms.len(), ACCENT),
                (
                    "Favorites",
                    self.roms.iter().filter(|rom| rom.favorite).count(),
                    ACCENT_GREEN,
                ),
                (
                    "Emulators",
                    self.emulators.len(),
                    Color32::from_rgb(191, 157, 255),
                ),
                (
                    "Platforms",
                    self.platforms.len(),
                    Color32::from_rgb(255, 188, 105),
                ),
            ] {
                card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(ui, |ui| {
                    ui.label(RichText::new(label).small().color(TEXT_MUTED));
                    ui.label(
                        RichText::new(value.to_string())
                            .size(24.0)
                            .strong()
                            .color(color),
                    );
                });
            }
        });

        ui.add_space(12.0);
        let visible_roms = self.modern_visible_roms();
        ui.columns(2, |columns| {
            columns[0].vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ROM LIBRARY")
                            .small()
                            .color(TEXT_MUTED)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(visible_roms.len().to_string()).color(ACCENT));
                    });
                });
                ui.add_space(8.0);
                ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    if visible_roms.is_empty() {
                        card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73)).show(
                            ui,
                            |ui| {
                                ui.label(
                                    RichText::new(if self.roms.is_empty() {
                                        "Import your first ROM to start building the library."
                                    } else {
                                        "No ROMs match the current search or filter."
                                    })
                                    .color(TEXT_MUTED),
                                );
                            },
                        );
                    } else {
                        ui.columns(2, |cards| {
                            for (index, rom) in visible_roms.iter().cloned().enumerate() {
                                Self::show_modern_rom_card(self, &mut cards[index % 2], rom);
                                cards[index % 2].add_space(10.0);
                            }
                        });
                    }
                });
            });
            columns[1].vertical(|ui| {
                self.show_modern_details(ui);
                self.show_modern_emulators(ui);
            });
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Home").clicked() {
                if let Some(home) = &self.home_dir {
                    self.current_dir = home.clone();
                    let _ = self.save_app_state();
                }
            }
            if ui.button("Go up").clicked() && self.current_dir.pop() {
                let _ = self.save_app_state();
            }
            if ui.button("Refresh library").clicked() {
                let _ = self.refresh_from_db();
            }
            ui.label(RichText::new(&self.status).small().color(TEXT_MUTED));
        });
        if !self.current_dir.as_os_str().is_empty() {
            ui.label(
                RichText::new(format!("Current directory: {}", self.current_dir.display()))
                    .small()
                    .color(TEXT_MUTED),
            );
        }
    }

    #[allow(dead_code)]
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
                            .button(if rom.favorite {
                                "\u{2605} Favorite"
                            } else {
                                "\u{2606} Favorite"
                            })
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
        ui.heading("ROM Artwork");
        ui.label("Use a TheGamesDB API key to automatically download cover art.");
        ui.horizontal(|ui| {
            ui.label("API key:");
            ui.add(
                TextEdit::singleline(&mut self.artwork_api_key)
                    .password(true)
                    .desired_width(360.0),
            );
            if ui.button("Save Artwork Settings").clicked() {
                match self.save_app_state() {
                    Ok(()) => {
                        self.status = "Artwork settings saved.".to_string();
                        self.auto_fetch_missing_artwork();
                    }
                    Err(error) => {
                        self.status = format!("Could not save artwork settings: {error}");
                    }
                }
            }
        });
        ui.label(
            RichText::new(
                "Artwork is cached locally and can also be fetched from a selected ROM's details.",
            )
            .small()
            .color(TEXT_MUTED),
        );
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

fn append_launch_arguments(
    command: &mut Command,
    settings: &LaunchSettings,
    executable_path: &str,
) {
    if settings.borderless_enabled {
        command.arg(&settings.borderless_arg);
    }
    if settings.fullscreen_enabled {
        command.arg(&settings.fullscreen_arg);
    } else if executable_path.to_lowercase().contains("mgba") {
        // mGBA persists its last fullscreen state. Force the requested
        // windowed state when the profile disables fullscreen.
        command.arg("-C").arg("fullscreen=0");
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
        self.process_pending_auto_scale();
        if self.pending_auto_scale.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        Panel::left("modern_navigation")
            .resizable(false)
            .default_size(190.0)
            .frame(Frame::new().fill(PANEL_BACKGROUND).inner_margin(12))
            .show(ui, |ui| self.show_modern_sidebar(ui));
        Panel::top("modern_top_bar")
            .frame(Frame::new().fill(PANEL_BACKGROUND).inner_margin(14))
            .show(ui, |ui| self.show_modern_top_bar(ui));
        CentralPanel::default()
            .frame(Frame::new().fill(APP_BACKGROUND).inner_margin(18))
            .show(ui, |ui| match self.tab {
                Tab::Main => self.show_modern_main(ui),
                Tab::Upscaling => self.show_upscaling_tab(ui),
                Tab::Settings => {
                    ui.label(RichText::new("Settings").size(22.0).strong());
                    ui.label(
                        RichText::new("Configure launch behavior, folders, and updates")
                            .color(TEXT_MUTED),
                    );
                    ui.add_space(12.0);
                    card_frame(PANEL_BACKGROUND, Color32::from_rgb(42, 61, 73))
                        .show(ui, |ui| self.show_settings_tab(ui));
                }
            });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.save_app_state();
        self.stop_magpie();
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
    let mut viewport = options
        .viewport
        .with_title(window_title.clone())
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([1000.0, 650.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
    options.viewport = viewport;
    eframe::run_native(
        &window_title,
        options,
        Box::new(|cc| {
            configure_theme(&cc.egui_ctx);
            Ok(App::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| Box::new(std::io::Error::other(error.to_string())))?)
        }),
    )
}
