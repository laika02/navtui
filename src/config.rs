use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_DIR_NAME: &str = "navtui";
const LEGACY_CONFIG_DIR_NAME: &str = "subsonic-tui";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub server_url: String,
    pub username: String,
    #[serde(default)]
    pub always_hard_refresh_on_launch: bool,
    #[serde(default, alias = "open_on_auto_select")]
    pub expand_on_search_collapse: bool,
    #[serde(default = "default_true")]
    pub show_identity_label: bool,
    #[serde(default)]
    pub keybinds: KeybindsConfig,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KeybindsConfig {
    #[serde(default = "default_keybind_queue_mode_toggle")]
    pub queue_mode_toggle: Vec<String>,
    #[serde(default = "default_keybind_quit")]
    pub quit: Vec<String>,
    #[serde(default = "default_keybind_escape")]
    pub escape: Vec<String>,
    #[serde(default = "default_keybind_global_reset")]
    pub global_reset: Vec<String>,
    #[serde(default = "default_keybind_volume_down")]
    pub volume_down: Vec<String>,
    #[serde(default = "default_keybind_volume_up")]
    pub volume_up: Vec<String>,
    #[serde(default = "default_keybind_seek_back")]
    pub seek_back: Vec<String>,
    #[serde(default = "default_keybind_seek_forward")]
    pub seek_forward: Vec<String>,
    #[serde(default = "default_keybind_search")]
    pub search: Vec<String>,
    #[serde(default = "default_keybind_tab_artists")]
    pub tab_artists: Vec<String>,
    #[serde(default = "default_keybind_tab_albums")]
    pub tab_albums: Vec<String>,
    #[serde(default = "default_keybind_tab_songs")]
    pub tab_songs: Vec<String>,
    #[serde(default = "default_keybind_tab_cycle")]
    pub tab_cycle: Vec<String>,
    #[serde(default = "default_keybind_nav_up")]
    pub nav_up: Vec<String>,
    #[serde(default = "default_keybind_nav_down")]
    pub nav_down: Vec<String>,
    #[serde(default = "default_keybind_nav_left")]
    pub nav_left: Vec<String>,
    #[serde(default = "default_keybind_activate")]
    pub activate: Vec<String>,
    #[serde(default = "default_keybind_enqueue")]
    pub enqueue: Vec<String>,
    #[serde(default = "default_keybind_play_next")]
    pub play_next: Vec<String>,
    #[serde(default = "default_keybind_play_pause")]
    pub play_pause: Vec<String>,
    #[serde(default = "default_keybind_clear_queue")]
    pub clear_queue: Vec<String>,
    #[serde(default = "default_keybind_hard_refresh")]
    pub hard_refresh: Vec<String>,
    #[serde(default = "default_keybind_shuffle")]
    pub shuffle: Vec<String>,
    #[serde(default = "default_keybind_queue_back")]
    pub queue_back: Vec<String>,
    #[serde(default = "default_keybind_queue_forward")]
    pub queue_forward: Vec<String>,
    #[serde(default = "default_keybind_queue_remove")]
    pub queue_remove: Vec<String>,
    #[serde(default = "default_keybind_queue_reorder_toggle")]
    pub queue_reorder_toggle: Vec<String>,
    #[serde(default = "default_keybind_search_backspace")]
    pub search_backspace: Vec<String>,
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            queue_mode_toggle: default_keybind_queue_mode_toggle(),
            quit: default_keybind_quit(),
            escape: default_keybind_escape(),
            global_reset: default_keybind_global_reset(),
            volume_down: default_keybind_volume_down(),
            volume_up: default_keybind_volume_up(),
            seek_back: default_keybind_seek_back(),
            seek_forward: default_keybind_seek_forward(),
            search: default_keybind_search(),
            tab_artists: default_keybind_tab_artists(),
            tab_albums: default_keybind_tab_albums(),
            tab_songs: default_keybind_tab_songs(),
            tab_cycle: default_keybind_tab_cycle(),
            nav_up: default_keybind_nav_up(),
            nav_down: default_keybind_nav_down(),
            nav_left: default_keybind_nav_left(),
            activate: default_keybind_activate(),
            enqueue: default_keybind_enqueue(),
            play_next: default_keybind_play_next(),
            play_pause: default_keybind_play_pause(),
            clear_queue: default_keybind_clear_queue(),
            hard_refresh: default_keybind_hard_refresh(),
            shuffle: default_keybind_shuffle(),
            queue_back: default_keybind_queue_back(),
            queue_forward: default_keybind_queue_forward(),
            queue_remove: default_keybind_queue_remove(),
            queue_reorder_toggle: default_keybind_queue_reorder_toggle(),
            search_backspace: default_keybind_search_backspace(),
        }
    }
}

fn keybind_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn default_keybind_queue_mode_toggle() -> Vec<String> {
    keybind_vec(&["shift", "backtab"])
}

fn default_keybind_quit() -> Vec<String> {
    keybind_vec(&["q"])
}

fn default_keybind_escape() -> Vec<String> {
    keybind_vec(&["esc"])
}

fn default_keybind_global_reset() -> Vec<String> {
    keybind_vec(&["shift+esc"])
}

fn default_keybind_volume_down() -> Vec<String> {
    keybind_vec(&["["])
}

fn default_keybind_volume_up() -> Vec<String> {
    keybind_vec(&["]"])
}

fn default_keybind_seek_back() -> Vec<String> {
    keybind_vec(&[";"])
}

fn default_keybind_seek_forward() -> Vec<String> {
    keybind_vec(&["'"])
}

fn default_keybind_search() -> Vec<String> {
    keybind_vec(&["/"])
}

fn default_keybind_tab_artists() -> Vec<String> {
    keybind_vec(&["1"])
}

fn default_keybind_tab_albums() -> Vec<String> {
    keybind_vec(&["2"])
}

fn default_keybind_tab_songs() -> Vec<String> {
    keybind_vec(&["3"])
}

fn default_keybind_tab_cycle() -> Vec<String> {
    keybind_vec(&["tab"])
}

fn default_keybind_nav_up() -> Vec<String> {
    keybind_vec(&["up"])
}

fn default_keybind_nav_down() -> Vec<String> {
    keybind_vec(&["down"])
}

fn default_keybind_nav_left() -> Vec<String> {
    keybind_vec(&["left"])
}

fn default_keybind_activate() -> Vec<String> {
    keybind_vec(&["right", "enter"])
}

fn default_keybind_enqueue() -> Vec<String> {
    keybind_vec(&["a", "A"])
}

fn default_keybind_play_next() -> Vec<String> {
    keybind_vec(&["n"])
}

fn default_keybind_play_pause() -> Vec<String> {
    keybind_vec(&["space"])
}

fn default_keybind_clear_queue() -> Vec<String> {
    keybind_vec(&["c"])
}

fn default_keybind_hard_refresh() -> Vec<String> {
    keybind_vec(&["r"])
}

fn default_keybind_shuffle() -> Vec<String> {
    keybind_vec(&["s"])
}

fn default_keybind_queue_back() -> Vec<String> {
    keybind_vec(&[","])
}

fn default_keybind_queue_forward() -> Vec<String> {
    keybind_vec(&["."])
}

fn default_keybind_queue_remove() -> Vec<String> {
    keybind_vec(&["backspace"])
}

fn default_keybind_queue_reorder_toggle() -> Vec<String> {
    keybind_vec(&["space"])
}

fn default_keybind_search_backspace() -> Vec<String> {
    keybind_vec(&["backspace"])
}

pub fn load() -> Result<Option<Config>> {
    let path = config_path()?;
    if path.exists() {
        return load_from_path(&path).map(Some);
    }

    let legacy_path = legacy_config_path()?;
    if legacy_path.exists() {
        return load_from_path(&legacy_path).map(Some);
    }

    Ok(None)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    let dir = config_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create config directory {}", dir.display()))?;
    }

    let data = toml::to_string_pretty(cfg).context("failed to serialize config")?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open config file {}", path.display()))?;
    file.write_all(data.as_bytes())
        .with_context(|| format!("failed to write config file {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(())
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config directory")?;
    Ok(base.join(CONFIG_DIR_NAME))
}

fn legacy_config_path() -> Result<PathBuf> {
    Ok(legacy_config_dir()?.join("config.toml"))
}

fn legacy_config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config directory")?;
    Ok(base.join(LEGACY_CONFIG_DIR_NAME))
}

fn load_from_path(path: &PathBuf) -> Result<Config> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;
    toml::from_str(&raw).context("failed to parse config TOML")
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn show_identity_label_defaults_to_true_when_missing() {
        let cfg: Config = toml::from_str(
            r#"
server_url = "https://music.example.com"
username = "tester"
"#,
        )
        .expect("config should parse");

        assert!(cfg.show_identity_label);
    }

    #[test]
    fn show_identity_label_respects_explicit_false() {
        let cfg: Config = toml::from_str(
            r#"
server_url = "https://music.example.com"
username = "tester"
show_identity_label = false
"#,
        )
        .expect("config should parse");

        assert!(!cfg.show_identity_label);
    }

    #[test]
    fn keybinds_defaults_are_available_when_missing() {
        let cfg: Config = toml::from_str(
            r#"
server_url = "https://music.example.com"
username = "tester"
"#,
        )
        .expect("config should parse");

        assert_eq!(cfg.keybinds.quit, vec!["q".to_string()]);
        assert_eq!(
            cfg.keybinds.queue_mode_toggle,
            vec!["shift".to_string(), "backtab".to_string()]
        );
        assert_eq!(cfg.keybinds.play_next, vec!["n".to_string()]);
        assert_eq!(cfg.keybinds.seek_back, vec![";".to_string()]);
        assert_eq!(cfg.keybinds.seek_forward, vec!["'".to_string()]);
        assert_eq!(cfg.keybinds.hard_refresh, vec!["r".to_string()]);
    }

    #[test]
    fn keybind_partial_override_keeps_other_defaults() {
        let cfg: Config = toml::from_str(
            r#"
server_url = "https://music.example.com"
username = "tester"

[keybinds]
quit = ["ctrl+q"]
"#,
        )
        .expect("config should parse");

        assert_eq!(cfg.keybinds.quit, vec!["ctrl+q".to_string()]);
        assert_eq!(cfg.keybinds.tab_cycle, vec!["tab".to_string()]);
    }
}
