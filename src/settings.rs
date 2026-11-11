//! The app's settings, kept in `$XDG_CONFIG_HOME/baliset/settings.json` (else
//! `~/.config/baliset/settings.json`).
//!
//! To add a setting, add a field to [`Settings`] and its default to `Default`. Reading is
//! [`get`], changing is [`update`]. A missing file, a missing key or a key whose value has the
//! wrong type each fall back to the default for just that setting; keys this version doesn't
//! know are kept and written back, so a newer version's settings survive an older one. Every
//! change rewrites the file through a temporary file and a rename.
//!
//! Tests and headless renders (`--snap`, `--script`) keep settings in memory only and start
//! from the defaults, so they neither read nor change the owner's file.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The UI's zoom, 1.0 for 100%.
    pub zoom: f32,
    pub audio_quality: AudioQuality,
    /// Even out loudness between tracks (Spotify's normalisation).
    pub normalize_volume: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { zoom: 1.0, audio_quality: AudioQuality::VeryHigh, normalize_volume: true }
    }
}

impl Settings {
    /// Values a hand-edited file may have pushed out of range, brought back in.
    fn sanitized(mut self) -> Self {
        let default = Self::default();
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            self.zoom = default.zoom;
        }
        self.zoom = self.zoom.clamp(0.25, 5.0);
        self
    }
}

/// The bitrate tracks stream at, as Spotify's clients name the choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioQuality {
    Low,
    Normal,
    #[default]
    VeryHigh,
}

impl AudioQuality {
    pub const ALL: [AudioQuality; 3] = [AudioQuality::Low, AudioQuality::Normal, AudioQuality::VeryHigh];

    pub fn kbps(self) -> u32 {
        match self {
            Self::Low => 96,
            Self::Normal => 160,
            Self::VeryHigh => 320,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::VeryHigh => "Very high",
        }
    }
}

struct Store {
    settings: Settings,
    /// As read at startup: what the parts that only read settings once are running with.
    launch: Settings,
    /// Keys this version doesn't know, written back unchanged.
    unknown: Map<String, Value>,
    /// `None` keeps everything in memory.
    path: Option<PathBuf>,
}

impl Store {
    fn open(path: Option<PathBuf>) -> Self {
        let file = path.as_deref().and_then(read_object).unwrap_or_default();
        let (settings, unknown) = parse(file);
        Self { launch: settings.clone(), settings, unknown, path }
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let mut map = self.unknown.clone();
        if let Value::Object(known) = serde_json::to_value(&self.settings)? {
            map.extend(known);
        }
        let mut json = serde_json::to_vec_pretty(&Value::Object(map))?;
        json.push(b'\n');
        write_atomic(path, &json)
    }
}

fn read_object(path: &Path) -> Option<Map<String, Value>> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(Value::Object(map)) => Some(map),
        _ => {
            log::warn!("{} isn't a JSON object; using the default settings", path.display());
            None
        }
    }
}

/// The settings `file` holds, each key that doesn't fit falling back alone, and the keys it
/// holds that [`Settings`] doesn't have.
fn parse(file: Map<String, Value>) -> (Settings, Map<String, Value>) {
    let Ok(Value::Object(mut merged)) = serde_json::to_value(Settings::default()) else {
        return (Settings::default(), file);
    };
    let mut unknown = Map::new();
    for (key, value) in file {
        let Some(previous) = merged.get(&key).cloned() else {
            unknown.insert(key, value);
            continue;
        };
        merged.insert(key.clone(), value);
        if serde_json::from_value::<Settings>(Value::Object(merged.clone())).is_err() {
            log::warn!("ignoring the setting {key:?}: it doesn't hold the right kind of value");
            merged.insert(key, previous);
        }
    }
    let settings = serde_json::from_value::<Settings>(Value::Object(merged)).unwrap_or_default();
    (settings.sanitized(), unknown)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map_or_else(|| "settings.json".into(), |n| n.to_string_lossy().into_owned());
    let temporary = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Where the settings file lives.
pub fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("baliset").join("settings.json"))
}

fn store() -> MutexGuard<'static, Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            let in_memory = cfg!(test) || rune::is_headless();
            Mutex::new(Store::open(if in_memory { None } else { path() }))
        })
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The current settings.
pub fn get() -> Settings {
    store().settings.clone()
}

/// The settings as they were when the app started.
pub fn at_launch() -> Settings {
    store().launch.clone()
}

/// Change the settings and save them if anything changed.
pub fn update(change: impl FnOnce(&mut Settings)) {
    let mut store = store();
    let before = store.settings.clone();
    change(&mut store.settings);
    store.settings = store.settings.clone().sanitized();
    if store.settings != before
        && let Err(error) = store.save()
    {
        log::warn!("can't save the settings: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(json: &str) -> Map<String, Value> {
        serde_json::from_str(json).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baliset-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("baliset").join("settings.json")
    }

    #[test]
    fn a_missing_or_empty_file_gives_the_defaults() {
        assert_eq!(Store::open(Some(scratch("missing"))).settings, Settings::default());
        assert_eq!(parse(Map::new()).0, Settings::default());
        assert_eq!(Settings::default().audio_quality.kbps(), 320);
    }

    #[test]
    fn a_wrong_value_falls_back_alone_and_unknown_keys_are_kept() {
        let (settings, unknown) = parse(object(r#"{"zoom": "big", "audio_quality": "low", "theme": "sepia"}"#));
        assert_eq!(settings, Settings { zoom: 1.0, audio_quality: AudioQuality::Low, ..Settings::default() });
        assert_eq!(unknown, object(r#"{"theme": "sepia"}"#));
    }

    #[test]
    fn an_out_of_range_zoom_is_brought_back() {
        assert_eq!(parse(object(r#"{"zoom": 40}"#)).0.zoom, 5.0);
        assert_eq!(parse(object(r#"{"zoom": -1}"#)).0.zoom, 1.0);
    }

    #[test]
    fn a_change_is_written_whole_and_reads_back_with_unknown_keys() {
        let path = scratch("roundtrip");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"zoom": 1.25, "later_feature": [1, 2]}"#).unwrap();
        let mut store = Store::open(Some(path.clone()));
        assert_eq!(store.settings.zoom, 1.25);
        store.settings.normalize_volume = false;
        store.save().unwrap();
        let reread = Store::open(Some(path.clone()));
        assert_eq!(reread.settings, Settings { zoom: 1.25, normalize_volume: false, ..Settings::default() });
        assert_eq!(reread.unknown, object(r#"{"later_feature": [1, 2]}"#));
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("settings.json")], "no temporary file is left behind");
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_file_that_isnt_an_object_gives_the_defaults() {
        let path = scratch("garbage");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[not settings").unwrap();
        assert_eq!(Store::open(Some(path.clone())).settings, Settings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
