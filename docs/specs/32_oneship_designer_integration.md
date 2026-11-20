# Spec 32: One-Ship Designer Integration (`desigma` Fork & MCP Gateway)

## 1. Overview
ONE-SHIP integrates a native design editor (**One-Ship Designer**, binary `oneship-designer`, based on `desigma`) as a decoupled sibling repository (`<sibling-repos>/desigma`) paired with a per-project MCP stdio pool and HTTP gateway in `oneship-daemon`.

---

## 2. Repository & Branching Topology
- **Upstream:** `https://github.com/deepfounder-ai/desigma.git`
- **Origin (Fork):** `git@github.com:ThePartyAcolyte/desigma.git`
- **`main`:** Tracks `upstream/main` cleanly without ONE-SHIP-specific changes.
- **Agnostic Bugfix / Feature Branches (`fix/*`, `feat/*`):** Branched from `main` for clean upstream Pull Requests (e.g. `fix/async-file-dialog`, `fix/svg-resize`).
- **`oneship`:** Integration branch based on `main` + agnostic fixes + ONE-SHIP customizations:
  - Rebranded as **One-Ship Designer** (`APP_TITLE = "One-Ship Designer"`, `[[bin]] name = "oneship-designer"`, `assets/icon/oneship-designer-256.png`, explicit `"is based on desigma"` attribution in *About* per `TRADEMARKS.md`).
  - Headless `--preview-json` renderer (with Google Fonts loading via `desigma_fonts::ensure_fonts` + `desigma_export::register_font`) and `--no-bridge` flag for isolated preview subprocesses.
  - `Mode::File` fallback in `desigma-mcp` when the GUI editor is not running.

### 2.1 Workflow: Upstream Sync, PRs & Branding Preservation
1. **Sync `main` from upstream:**
   ```sh
   git fetch upstream
   git checkout main
   git merge --ff-only upstream/main
   git push origin main
   ```
2. **Contribute agnostic fixes/features upstream:**
   - Branch from `main` (`git checkout -b fix/<topic> main` or `feat/<topic>`). Keep commits strictly free of ONE-SHIP branding or ONE-SHIP-only flags.
   - Verify against upstream `CONTRIBUTING.md` checks (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt` on touched files).
   - Push to `origin` (`git push -u origin fix/<topic>`) and open a Pull Request against `deepfounder-ai/desigma`.
3. **Update `oneship` without losing branding:**
   - Merge `main` (and any local `fix/*` branches prior to upstream merge) into `oneship`:
     ```sh
     git checkout oneship
     git merge main
     ```
   - Because branding is isolated in dedicated commits (`Cargo.toml`, `theme.rs`, `home/dialog.rs`, `main.rs`, `assets/icon/oneship-designer-256.png`), non-branding upstream changes merge cleanly. If upstream touches those exact branding lines/files, keep ONE-SHIP's branding (`git checkout --ours ...`) and re-verify `"is based on desigma"` compliance per `TRADEMARKS.md`.
   - Rebuild the release binaries (`cargo build --release -p desigma-app -p desigma-mcp`); `oneship-daemon` picks up `oneship-designer` on the next launch or preview render without rebuilding ONE-SHIP.

---

## 3. Daemon & Binary Resolution (`kopeng/desigma.rs`, `config.rs`)
- `oneship-daemon` does **not** compile `desigma` during its own build. It resolves external binaries and parameters dynamically at runtime via `[plugins.desigma]` in `config.rs` (`enabled`, `app_binary_path`, `mcp_binary_path`, `design_dir`, `enable_bridge` defaulting to `false`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, `max_snapshots_per_asset_count`).
- **Daemon Commands:** `get_project_desigma_status`, `toggle_project_desigma_plugin` (also symlinks/cleans `oneship-desigma` skill), `launch_project_desigma` (spawns `oneship-designer` with `--bridge` or `--no-bridge` according to `enable_bridge` on an empty Hyprland workspace via `desktop.rs`), and `render_desigma_preview` (spawns `oneship-designer --preview-json` backed by `kopeng/preview_cache.rs` SHA-256 disk cache and `hardwarePreviewStore.ts`).
- **File Watcher & Auto-Snapshots:** A `notify` watcher monitors `<project>/<design_dir>` for `.desigma` / `.fig` changes, emits `desigma-file-changed` over `/api/events`, and triggers `try_create_auto_domain_snapshot`.
- **MCP Gateway:** `/mcp/desigma?project_hex=...` routes tool calls to a per-project `desigma-mcp` stdio process pool with Rule 11 caller attestation.
