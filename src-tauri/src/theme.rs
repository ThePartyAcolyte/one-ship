//! Theme palette resolution: built-in IDE presets (dark & light) plus live Omarchy desktop sync.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::api::ApiError;

/// Option name for live synchronization with the active Omarchy desktop theme.
pub const AUTO_OMARCHY_THEME: &str = "Auto (Omarchy)";

/// Default built-in theme used when Omarchy is not present on the host.
pub const DEFAULT_FALLBACK_THEME: &str = "Tokyo Night";

/// Domain error returned when resolving or parsing a theme palette fails.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    /// `$HOME` could not be resolved from the host environment.
    #[error("Cannot resolve the user home directory for the Omarchy theme state")]
    HomeDirUnavailable,

    /// Reading `colors.toml` from disk failed.
    #[error("Cannot read Omarchy theme file '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Parsing `colors.toml` failed or a required color key is missing.
    #[error("Invalid Omarchy theme file '{path}': {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    /// Configured theme name is not a recognized preset or valid string.
    #[error("Unknown theme '{name}' configured in appearance.theme")]
    UnknownTheme { name: String },
}

impl ApiError for ThemeError {
    fn status(&self) -> StatusCode {
        match self {
            Self::UnknownTheme { .. } => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::HomeDirUnavailable => "ThemeError::HomeDirUnavailable",
            Self::Io { .. } => "ThemeError::Io",
            Self::Parse { .. } => "ThemeError::Parse",
            Self::UnknownTheme { .. } => "ThemeError::UnknownTheme",
        }
    }
}

/// Canonical color keys defined by an Omarchy theme's `colors.toml`.
#[derive(Deserialize, Debug)]
struct OmarchyColorsToml {
    mode: String,
    background: String,
    foreground: String,
    muted: String,
    lighter_background: String,
    accent: String,
    green: String,
    yellow: String,
    red: String,
    cyan: String,
}

/// Semantic theme palette exposed to the ONE-SHIP interface.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, ts_rs::TS)]
#[ts(export)]
pub struct ThemeColors {
    pub mode: String,
    pub auto_sync: bool,
    pub bg_color: String,
    pub fg_color: String,
    pub dim_color: String,
    pub border_color: String,
    pub accent_color: String,
    pub ok_color: String,
    pub warn_color: String,
    pub error_color: String,
    pub info_color: String,
}

struct ThemePreset {
    name: &'static str,
    mode: &'static str,
    bg_color: &'static str,
    fg_color: &'static str,
    dim_color: &'static str,
    border_color: &'static str,
    accent_color: &'static str,
    ok_color: &'static str,
    warn_color: &'static str,
    error_color: &'static str,
    info_color: &'static str,
}

const THEME_PRESETS: &[ThemePreset] = &[
    // Dark IDE / terminal palettes
    ThemePreset {
        name: "Tokyo Night",
        mode: "dark",
        bg_color: "#1a1b26",
        fg_color: "#a9b1d6",
        dim_color: "#565f89",
        border_color: "#24283b",
        accent_color: "#7aa2f7",
        ok_color: "#9ece6a",
        warn_color: "#e0af68",
        error_color: "#f7768e",
        info_color: "#7dcfff",
    },
    ThemePreset {
        name: "Catppuccin Mocha",
        mode: "dark",
        bg_color: "#1e1e2e",
        fg_color: "#cdd6f4",
        dim_color: "#6c7086",
        border_color: "#313244",
        accent_color: "#89b4fa",
        ok_color: "#a6e3a1",
        warn_color: "#f9e2af",
        error_color: "#f38ba8",
        info_color: "#94e2d5",
    },
    ThemePreset {
        name: "Gruvbox Dark",
        mode: "dark",
        bg_color: "#282828",
        fg_color: "#ebdbb2",
        dim_color: "#928374",
        border_color: "#3c3836",
        accent_color: "#fabd2f",
        ok_color: "#b8bb26",
        warn_color: "#fe8019",
        error_color: "#fb4934",
        info_color: "#83a598",
    },
    ThemePreset {
        name: "Nord",
        mode: "dark",
        bg_color: "#2e3440",
        fg_color: "#eceff4",
        dim_color: "#616e88",
        border_color: "#3b4252",
        accent_color: "#88c0d0",
        ok_color: "#a3be8c",
        warn_color: "#ebcb8b",
        error_color: "#bf616a",
        info_color: "#81a1c1",
    },
    ThemePreset {
        name: "Rose Pine",
        mode: "dark",
        bg_color: "#191724",
        fg_color: "#e0def4",
        dim_color: "#6e6a86",
        border_color: "#26233a",
        accent_color: "#c4a7e7",
        ok_color: "#9ccfd8",
        warn_color: "#f6c177",
        error_color: "#eb6f92",
        info_color: "#31748f",
    },
    ThemePreset {
        name: "Kanagawa",
        mode: "dark",
        bg_color: "#1f1f28",
        fg_color: "#dcd7ba",
        dim_color: "#727169",
        border_color: "#2a2a37",
        accent_color: "#7e9cd8",
        ok_color: "#98bb6c",
        warn_color: "#e6c384",
        error_color: "#e46876",
        info_color: "#7fb4ca",
    },
    ThemePreset {
        name: "Everforest Dark",
        mode: "dark",
        bg_color: "#2d353b",
        fg_color: "#d3c6aa",
        dim_color: "#859289",
        border_color: "#3d484d",
        accent_color: "#a7c080",
        ok_color: "#a7c080",
        warn_color: "#dbbc7f",
        error_color: "#e67e80",
        info_color: "#7fbbb3",
    },
    ThemePreset {
        name: "One Dark",
        mode: "dark",
        bg_color: "#282c34",
        fg_color: "#abb2bf",
        dim_color: "#5c6370",
        border_color: "#3e4451",
        accent_color: "#61afef",
        ok_color: "#98c379",
        warn_color: "#e5c07b",
        error_color: "#e06c75",
        info_color: "#56b6c2",
    },
    ThemePreset {
        name: "Dracula",
        mode: "dark",
        bg_color: "#282a36",
        fg_color: "#f8f8f2",
        dim_color: "#6272a4",
        border_color: "#44475a",
        accent_color: "#bd93f9",
        ok_color: "#50fa7b",
        warn_color: "#f1fa8c",
        error_color: "#ff5555",
        info_color: "#8be9fd",
    },
    // Light IDE / terminal palettes
    ThemePreset {
        name: "Catppuccin Latte",
        mode: "light",
        bg_color: "#eff1f5",
        fg_color: "#4c4f69",
        dim_color: "#8c8fa1",
        border_color: "#ccd0da",
        accent_color: "#1e66f5",
        ok_color: "#40a02b",
        warn_color: "#df8e1d",
        error_color: "#d20f39",
        info_color: "#179299",
    },
    ThemePreset {
        name: "Tokyo Night Day",
        mode: "light",
        bg_color: "#e1e2e7",
        fg_color: "#3760bf",
        dim_color: "#848cb5",
        border_color: "#c4c8da",
        accent_color: "#2e7de9",
        ok_color: "#587539",
        warn_color: "#8c6c3e",
        error_color: "#f52a65",
        info_color: "#007197",
    },
    ThemePreset {
        name: "Gruvbox Light",
        mode: "light",
        bg_color: "#fbf1c7",
        fg_color: "#3c3836",
        dim_color: "#7c6f64",
        border_color: "#d5c4a1",
        accent_color: "#b57614",
        ok_color: "#79740e",
        warn_color: "#af3a03",
        error_color: "#9d0006",
        info_color: "#427b58",
    },
    ThemePreset {
        name: "Rose Pine Dawn",
        mode: "light",
        bg_color: "#faf4ed",
        fg_color: "#575279",
        dim_color: "#9893a5",
        border_color: "#dfdad9",
        accent_color: "#907aa9",
        ok_color: "#56949f",
        warn_color: "#ea9d34",
        error_color: "#b4637a",
        info_color: "#286983",
    },
    ThemePreset {
        name: "Solarized Light",
        mode: "light",
        bg_color: "#fdf6e3",
        fg_color: "#657b83",
        dim_color: "#93a1a1",
        border_color: "#eee8d5",
        accent_color: "#268bd2",
        ok_color: "#859900",
        warn_color: "#b58900",
        error_color: "#dc322f",
        info_color: "#2aa198",
    },
];

fn omarchy_colors_path() -> Result<PathBuf, ThemeError> {
    let home_dir = dirs::home_dir().ok_or(ThemeError::HomeDirUnavailable)?;
    Ok(home_dir.join(".local/state/omarchy/current/theme/colors.toml"))
}

/// Returns `true` when Omarchy's active theme state file exists on the host.
pub fn is_omarchy_available() -> bool {
    omarchy_colors_path().is_ok_and(|p| p.is_file())
}

/// Returns the ordered list of theme options valid for the current host environment.
pub fn available_theme_names() -> Vec<String> {
    let mut names = Vec::with_capacity(THEME_PRESETS.len() + 1);
    if is_omarchy_available() {
        names.push(AUTO_OMARCHY_THEME.to_string());
    }
    for preset in THEME_PRESETS {
        names.push(preset.name.to_string());
    }
    names
}

/// Returns the default theme name for the current host environment.
pub fn default_theme_name() -> &'static str {
    if is_omarchy_available() {
        AUTO_OMARCHY_THEME
    } else {
        DEFAULT_FALLBACK_THEME
    }
}

/// Parses an Omarchy `colors.toml` file into [`ThemeColors`] using a strict 1:1 key mapping.
pub fn parse_colors_toml(path: &Path) -> Result<ThemeColors, ThemeError> {
    let content = fs::read_to_string(path).map_err(|source| ThemeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_colors_toml_str(&content, path)
}

fn parse_colors_toml_str(content: &str, path: &Path) -> Result<ThemeColors, ThemeError> {
    let parsed: OmarchyColorsToml = toml::from_str(content).map_err(|source| ThemeError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    let mode = match parsed.mode.as_str() {
        "light" => "light".to_string(),
        _ => "dark".to_string(),
    };

    Ok(ThemeColors {
        mode,
        auto_sync: true,
        bg_color: parsed.background,
        fg_color: parsed.foreground,
        dim_color: parsed.muted,
        border_color: parsed.lighter_background,
        accent_color: parsed.accent,
        ok_color: parsed.green,
        warn_color: parsed.yellow,
        error_color: parsed.red,
        info_color: parsed.cyan,
    })
}

/// Reads the active Omarchy theme's palette from `$HOME/.local/state/omarchy/current/theme/colors.toml`.
pub fn get_omarchy_colors() -> Result<ThemeColors, ThemeError> {
    let state_active_colors = omarchy_colors_path()?;
    parse_colors_toml(&state_active_colors)
}

/// Resolves a theme by name (`Auto (Omarchy)` or any built-in preset).
pub fn resolve_theme_by_name(name: &str) -> Result<ThemeColors, ThemeError> {
    if name == AUTO_OMARCHY_THEME {
        return get_omarchy_colors();
    }

    let preset = THEME_PRESETS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ThemeError::UnknownTheme {
            name: name.to_string(),
        })?;

    Ok(ThemeColors {
        mode: preset.mode.to_string(),
        auto_sync: false,
        bg_color: preset.bg_color.to_string(),
        fg_color: preset.fg_color.to_string(),
        dim_color: preset.dim_color.to_string(),
        border_color: preset.border_color.to_string(),
        accent_color: preset.accent_color.to_string(),
        ok_color: preset.ok_color.to_string(),
        warn_color: preset.warn_color.to_string(),
        error_color: preset.error_color.to_string(),
        info_color: preset.info_color.to_string(),
    })
}

/// Daemon command `get_active_theme`: resolves `plugins.appearance.theme` from global config.
pub fn get_active_theme() -> Result<ThemeColors, ThemeError> {
    let configured = crate::config::resolve_effective_value(None, None, "appearance", "theme");
    let theme_name = configured
        .as_str()
        .ok_or_else(|| ThemeError::UnknownTheme {
            name: configured.to_string(),
        })?;
    resolve_theme_by_name(theme_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colors_toml_maps_canonical_keys_without_order_collisions() {
        let sample = r##"
mode = "dark"
accent = "#78824b"
selection = "#383838"
muted = "#666666"
background = "#222222"
dark_background = "#191919"
lighter_background = "#2c2c2c"
foreground = "#c2c2b0"
dark_foreground = "#555555"
red = "#685742"
yellow = "#b36d43"
orange = "#8d6242"
green = "#5f875f"
cyan = "#c9a554"
"##;
        let colors = parse_colors_toml_str(sample, Path::new("colors.toml")).unwrap();
        assert_eq!(colors.mode, "dark");
        assert!(colors.auto_sync);
        assert_eq!(colors.bg_color, "#222222");
        assert_eq!(colors.fg_color, "#c2c2b0");
        assert_eq!(colors.dim_color, "#666666");
        assert_eq!(colors.border_color, "#2c2c2c");
        assert_eq!(colors.accent_color, "#78824b");
        assert_eq!(colors.ok_color, "#5f875f");
        assert_eq!(colors.warn_color, "#b36d43");
        assert_eq!(colors.error_color, "#685742");
        assert_eq!(colors.info_color, "#c9a554");
    }

    #[test]
    fn all_presets_have_valid_hex_colors_and_modes() {
        for preset in THEME_PRESETS {
            let resolved = resolve_theme_by_name(preset.name).unwrap();
            assert!(!resolved.auto_sync);
            assert!(resolved.mode == "dark" || resolved.mode == "light");
            for hex in [
                &resolved.bg_color,
                &resolved.fg_color,
                &resolved.dim_color,
                &resolved.border_color,
                &resolved.accent_color,
                &resolved.ok_color,
                &resolved.warn_color,
                &resolved.error_color,
                &resolved.info_color,
            ] {
                assert!(hex.starts_with('#') && hex.len() == 7, "Invalid hex {hex} in {}", preset.name);
            }
        }
    }

    #[test]
    fn resolve_unknown_theme_returns_typed_error() {
        let err = resolve_theme_by_name("NonExistentTheme").unwrap_err();
        assert!(matches!(err, ThemeError::UnknownTheme { .. }));
    }
}
