//! Configurações do usuário (JSON validado ao carregar).
use crate::downloader::{Mode, AUDIO_FORMATS, BROWSERS, QUALITIES};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folder: String,
    pub mode: Mode,
    pub quality: String,
    pub audio_format: String,
    pub playlist: bool,
    pub channel: String,
    pub auto_update: bool,
    pub cookies_browser: String,
    pub last_check: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            folder: paths::default_download_dir().to_string_lossy().to_string(),
            mode: Mode::Video,
            quality: "1080".to_string(),
            audio_format: "mp3".to_string(),
            playlist: false,
            channel: "stable".to_string(),
            auto_update: true,
            cookies_browser: String::new(),
            last_check: 0.0,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        paths::config_dir().join("settings.json")
    }

    pub fn load() -> Settings {
        let mut s = fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
            .unwrap_or_default();
        s.sanitize();
        s
    }

    /// Qualquer valor fora das listas permitidas volta ao padrão.
    pub fn sanitize(&mut self) {
        let d = Settings::default();
        if !QUALITIES.contains(&self.quality.as_str()) {
            self.quality = d.quality;
        }
        if !AUDIO_FORMATS.contains(&self.audio_format.as_str()) {
            self.audio_format = d.audio_format;
        }
        if !BROWSERS.contains(&self.cookies_browser.as_str()) {
            self.cookies_browser = String::new();
        }
        if self.channel != "stable" && self.channel != "nightly" {
            self.channel = d.channel;
        }
        if self.folder.trim().is_empty() {
            self.folder = d.folder;
        }
        if !self.last_check.is_finite() {
            self.last_check = 0.0;
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, text).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_values_fall_back() {
        let mut s: Settings = serde_json::from_str(
            r#"{"quality":"evil","cookies_browser":"--exec","channel":"x","audio_format":"zzz"}"#,
        )
        .unwrap();
        s.sanitize();
        assert_eq!(s.quality, "1080");
        assert_eq!(s.cookies_browser, "");
        assert_eq!(s.channel, "stable");
        assert_eq!(s.audio_format, "mp3");
    }

    #[test]
    fn roundtrip() {
        let mut s = Settings::default();
        s.quality = "720".into();
        let t = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&t).unwrap();
        assert_eq!(back.quality, "720");
    }
}
