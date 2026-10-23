//! `~/.config/vista/config.toml` and the directories vista uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where vista reads configuration and writes caches and logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_file: PathBuf,
}

impl Paths {
    /// Resolves paths from the environment:
    /// `VISTA_CONFIG_DIR` › `$XDG_CONFIG_HOME/vista` › `~/.config/vista`, and
    /// `VISTA_CACHE_DIR` › the platform cache dir (`~/Library/Caches/vista` on macOS).
    pub fn from_env() -> Paths {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_dir = env("VISTA_CONFIG_DIR")
            .or_else(|| env("XDG_CONFIG_HOME").map(|p| p.join("vista")))
            .unwrap_or_else(|| home.join(".config").join("vista"));
        let cache_dir = env("VISTA_CACHE_DIR")
            .or_else(|| dirs::cache_dir().map(|p| p.join("vista")))
            .unwrap_or_else(|| home.join(".cache").join("vista"));
        let log_file = env("VISTA_LOG_FILE").unwrap_or_else(|| cache_dir.join("vista.log"));
        Paths {
            config_dir,
            cache_dir,
            log_file,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageProtocol {
    /// Detect what the terminal supports.
    #[default]
    Auto,
    Kitty,
    Halfblocks,
    Off,
}

impl ImageProtocol {
    pub fn parse(s: &str) -> Option<ImageProtocol> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "kitty" => Some(Self::Kitty),
            "halfblocks" => Some(Self::Halfblocks),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorConfigMode {
    /// Neovim loads the user's own `init.lua`.
    #[default]
    User,
    /// Neovim starts with `--clean` (no user config, no plugins).
    Minimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImagesConfig {
    pub protocol: ImageProtocol,
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            protocol: ImageProtocol::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorConfig {
    pub config: EditorConfigMode,
    /// Push vista's palette into Neovim so the editor matches the look.
    pub theme_sync: bool,
    /// Neovim executable.
    pub nvim: String,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            config: EditorConfigMode::User,
            theme_sync: true,
            nvim: "nvim".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TreeConfig {
    /// Show git-ignored files (dimmed).
    pub show_ignored: bool,
    /// Colour folders that contain changes.
    pub folder_colours: bool,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            show_ignored: true,
            folder_colours: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseConfig {
    /// Copy a mouse selection when the button is released (as most
    /// terminals do); off, it waits for Ctrl-C / Cmd-C / y.
    pub copy_on_select: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self { copy_on_select: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Id of the look (built-in or a file in `<config>/themes/`).
    pub theme: String,
    pub images: ImagesConfig,
    pub editor: EditorConfig,
    pub tree: TreeConfig,
    pub mouse: MouseConfig,
    /// Key overrides: `"ctrl+p" = "goto-file"`.
    pub keys: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "darcula".into(),
            images: ImagesConfig::default(),
            editor: EditorConfig::default(),
            tree: TreeConfig::default(),
            mouse: MouseConfig::default(),
            keys: BTreeMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl Config {
    /// Loads `config.toml` (a missing file means defaults), then applies
    /// environment overrides (`VISTA_IMAGE_PROTOCOL`, `VISTA_THEME`).
    pub fn load(paths: &Paths) -> Result<Config, ConfigError> {
        let mut config = Self::load_file(&paths.config_file())?;
        if let Ok(p) = std::env::var("VISTA_IMAGE_PROTOCOL")
            && let Some(p) = ImageProtocol::parse(&p)
        {
            config.images.protocol = p;
        }
        if let Ok(t) = std::env::var("VISTA_THEME")
            && !t.is_empty()
        {
            config.theme = t;
        }
        Ok(config)
    }

    pub fn load_file(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(src) => toml::from_str(&src).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

/// A `config.toml` with every setting at its default, explained; written
/// when Open Settings finds no file yet.
pub const TEMPLATE: &str = r#"# vista settings. Changes apply when vista starts again.
# Every value below is the default; delete a line to keep the default.

theme = "darcula"                    # darcula, quiet, paper, kanagawa, nord, … (herdr's themes), "herdr" to follow herdr, or a file in themes/

[images]
protocol = "auto"                    # auto, kitty, halfblocks, off

[editor]
config = "user"                      # user: your init.lua; minimal: nvim --clean
theme_sync = true                    # paint Neovim with the active look
nvim = "nvim"

[tree]
show_ignored = true                  # show git-ignored files (Alt-I toggles)
folder_colours = true                # colour folders that contain changes

[mouse]
copy_on_select = true                # copy a mouse selection on release

[keys]
# "ctrl+o" = "goto-file"             # bind a key to an action
# "q" = "none"                       # unbind a key
"#;

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn the_settings_template_is_the_defaults() {
        let parsed: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load_file(&dir.path().join("config.toml")).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.theme, "darcula");
        assert!(c.tree.show_ignored);
    }

    #[test]
    fn parses_partial_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "theme = \"quiet\"\n[images]\nprotocol = \"halfblocks\"\n[editor]\nconfig = \"minimal\"\n[keys]\n\"ctrl+o\" = \"goto-file\"\n",
        )
        .unwrap();
        let c = Config::load_file(&path).unwrap();
        assert_eq!(c.theme, "quiet");
        assert_eq!(c.images.protocol, ImageProtocol::Halfblocks);
        assert_eq!(c.editor.config, EditorConfigMode::Minimal);
        assert!(c.editor.theme_sync, "unset fields keep defaults");
        assert_eq!(c.keys.get("ctrl+o").map(String::as_str), Some("goto-file"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[tree]\nshow_ignord = false\n").unwrap();
        assert!(matches!(Config::load_file(&path), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn image_protocol_parsing() {
        assert_eq!(ImageProtocol::parse("Kitty"), Some(ImageProtocol::Kitty));
        assert_eq!(ImageProtocol::parse("sixel"), None);
    }
}
