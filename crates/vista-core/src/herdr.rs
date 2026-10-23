//! herdr's theme (`[theme]` in herdr's `config.toml`), for `theme = "herdr"`:
//! vista uses the look of the same name and follows herdr when it changes.

use std::path::{Path, PathBuf};

/// What herdr's `[theme]` says, with herdr's defaults for what's missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrTheme {
    pub name: String,
    /// Pick `dark_name` / `light_name` by the appearance instead of `name`.
    pub auto_switch: bool,
    pub dark_name: String,
    pub light_name: String,
}

impl Default for HerdrTheme {
    fn default() -> Self {
        Self {
            name: "catppuccin".into(),
            auto_switch: false,
            dark_name: "catppuccin".into(),
            light_name: "catppuccin-latte".into(),
        }
    }
}

impl HerdrTheme {
    /// The theme herdr shows, given whether the appearance is dark.
    pub fn current(&self, dark: bool) -> &str {
        match (self.auto_switch, dark) {
            (false, _) => &self.name,
            (true, true) => &self.dark_name,
            (true, false) => &self.light_name,
        }
    }

    /// Reads it from herdr's config; herdr's defaults when there is none
    /// (or it doesn't parse: herdr would complain about that itself).
    pub fn load(path: &Path) -> HerdrTheme {
        let mut t = HerdrTheme::default();
        let Some(theme) = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.parse::<toml::Table>().ok())
            .and_then(|mut v| v.remove("theme"))
        else {
            return t;
        };
        let text = |k: &str| theme.get(k).and_then(|v| v.as_str()).map(str::to_string);
        if let Some(n) = text("name") {
            t.name = n;
        }
        if let Some(n) = text("dark_name") {
            t.dark_name = n;
        }
        if let Some(n) = text("light_name") {
            t.light_name = n;
        }
        t.auto_switch = theme.get("auto_switch").and_then(|v| v.as_bool()).unwrap_or(false);
        t
    }
}

/// herdr's `config.toml`: `$XDG_CONFIG_HOME/herdr`, else `~/.config/herdr`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    Some(base.join("herdr").join("config.toml"))
}

/// Whether the system appearance is dark (macOS); elsewhere, dark.
pub fn system_dark() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "Dark")
}

/// The theme herdr shows now (reading its config and, for `auto_switch`, the
/// appearance).
pub fn current_theme() -> String {
    let theme = config_path().map(|p| HerdrTheme::load(&p)).unwrap_or_default();
    let dark = !theme.auto_switch || system_dark();
    theme.current(dark).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(src: &str) -> HerdrTheme {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, src).unwrap();
        HerdrTheme::load(&p)
    }

    #[test]
    fn reads_herdrs_theme_with_its_defaults() {
        let t = load(
            "onboarding = false\n[theme]\nname = \"kanagawa\"\nauto_switch = false\n[ui.toast]\ndelivery = \"terminal\"\n",
        );
        assert_eq!(t.name, "kanagawa");
        assert_eq!(t.current(true), "kanagawa");
        assert_eq!(t.current(false), "kanagawa");
        // No [theme], no file, or a broken one: herdr's defaults.
        assert_eq!(load("[ui]\nsidebar_width = 30\n"), HerdrTheme::default());
        assert_eq!(load("not = [toml"), HerdrTheme::default());
        let missing = HerdrTheme::load(Path::new("/nonexistent/herdr/config.toml"));
        assert_eq!(missing.current(true), "catppuccin");
    }

    #[test]
    fn auto_switch_picks_by_appearance() {
        let t = load(
            "[theme]\nname = \"nord\"\nauto_switch = true\ndark_name = \"kanagawa\"\nlight_name = \"kanagawa-lotus\"\n",
        );
        assert_eq!(t.current(true), "kanagawa");
        assert_eq!(t.current(false), "kanagawa-lotus");
        // Names left out are herdr's defaults.
        let t = load("[theme]\nauto_switch = true\n");
        assert_eq!((t.current(true), t.current(false)), ("catppuccin", "catppuccin-latte"));
    }
}
