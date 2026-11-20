# 2026-09-09: Omarchy Theme Synchronization Fix

## Overview
Resolved an issue where active Omarchy themes with multi-word names (such as "Osaka Jade") failed to sync colors, causing the background to remain near-black (`#030303`) instead of taking the theme's active background color (`#111c18`).

## Root Cause Analysis
1. `omarchy theme current` outputs title-cased theme names with spaces (e.g. `Osaka Jade`).
2. Theme directories on disk under `/usr/share/omarchy/themes/` and `~/.config/omarchy/themes/` use hyphenated slugs (`osaka-jade`).
3. The previous theme loader attempted to load `/usr/share/omarchy/themes/osaka jade/colors.toml`, which did not exist, triggering a fallback to hardcoded default values.
4. Omarchy maintains an active theme symlink at `~/.local/state/omarchy/current/theme/colors.toml` which was not being checked as a primary source.

## Changes Applied
- **`src-tauri/src/theme.rs`**:
  - Added primary check for active theme state symlink at `~/.local/state/omarchy/current/theme/colors.toml` and `~/.config/omarchy/current/theme/colors.toml`.
  - Added slugification fallback (`theme_name.replace(' ', "-").replace('_', "-")`) when resolving theme paths on disk.
  - Expanded `parse_colors_toml` key matching for aliases (`bg`, `color0`, `fg`, `color7`, `muted`, `dark_foreground`, `color8`, `selection_background`).
  - Added unit test `test_get_omarchy_colors` verifying retrieval of Osaka Jade `#111c18` background color.

## Verification
- Unit test `test_get_omarchy_colors` passed via `cargo test`.
