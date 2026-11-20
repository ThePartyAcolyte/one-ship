# Technical Debt Register

## 7. Multi-Agent Audit Trail & Distributed Collaboration Ledger
**Context:** Multi-agent collaboration currently logs delegations and return callbacks in local user storage (`~/.config/one-ship/delegations.json` and `logs/oneship.log`).
**Problem:** In distributed team environments, team members and DevOps automation need visibility into which agent executed which prompt and the verified task outcomes across multiple workstations. Committing raw audit trails directly into the Git repository (`.one-ship/audit/`) is not the ideal mechanism as it pollutes repository VCS history and falls outside the scope of the local workstation orchestrator.
**Deferred Solution:** Re-evaluate in a future maturity phase via a centralized synchronization mechanism (e.g. shared cloud database or dedicated team relay service) rather than committing audit logs to the Git tree. Kept out of scope for the current development phase.

## 22. Subprocess MCP Tool Discovery & Global Antigravity Config Synchronization (ACTIVE)
**Context:** Antigravity CLI (`agy`) discovers MCP servers exclusively from the machine-global configuration file (`~/.gemini/config/mcp_config.json`). Project-level plugins and `.agents/mcp_config.json` are not dynamically mounted for SSE servers unless registered in the global configuration.
**Problem:**
1. *Missing MCP Invocations in Subprocesses:* When `one-ship` was removed from `~/.gemini/config/mcp_config.json` on 2026-09-13 to prevent foreign IDE sessions from seeing internal tools, spawned subprocesses (`initializer`, `profiler-crafter`, `bosmang`) lost access to all platform MCP tools (`post_agent_response`, `resolve_initialization_flag`, `manage_todo_item`, `record_technical_debt`, `profile_service`).
2. *Cascading Terminal Output Trap:* Agents received `unknown tool: "post_agent_response"` and `unknown tool: "resolve_initialization_flag"`, falling back to dumping raw responses to terminal stdout. `supervisor.rs` only captured text deltas into internal ring buffers, leaving `agent_chat_turns` empty and the Chat Overlay blank, and triggering an infinite retry loop via stdin compliance reminders.
**Deferred Solution & Action Plan:**
1. Dynamically synchronize `one-ship` (`http://localhost:<mcp_port>/mcp`) into `~/.gemini/config/mcp_config.json` upon daemon startup, platform skill injection, and global configuration updates, preserving existing third-party servers.
2. Rely on `validate_agent_id` and `WORKSPACE_SCOPING_ERROR` in `mcp.rs` for deterministic foreign session rejection instead of un-mounting the global server.
3. Enhance `supervisor.rs` to buffer turn `agent_response` text deltas and ingest a fallback `agent_chat_turns` record if a turn concludes without `post_agent_response`.
4. Keep this debt item open until end-to-end multi-turn flag resolution is verified on Flag 27 (`fideiuris`).

## 23. Per-Agent Account Identity vs Global `agy` Keyring (F6, UNVERIFIED)
**Context:** `launch_agent` rewrites the single OS keyring entry (`service gemini, username antigravity`) under `keyring_lock()` before each `agy` spawn and calls `db_set_active_account`; `switch_account` does the same. See Spec 30 §12 and audit F6.
**Problem:**
1. *Active flag:* the "active" account in SQLite follows the last spawn, not any per-agent choice.
2. *Unverified premise:* the maintainer recalls that a running `agy` keeps the identity it started with while another `agy` is launched with a different account. This was not re-tested and may mean concurrent accounts already work. A token refresh by an older process writing back to the keyring is also untested.
**Deferred Solution:** Run the two-agent test (A with account X, B with account Y, compare each reported account/quota, repeat after A refreshes its token). If identities hold, stop flipping the active flag on spawn and derive the displayed account from the conversation/process. Otherwise follow Spec 30 §12 Option 1 and spike Option 2.

## 24. Gemini Embedding 2 Cloud Provider & Hybrid Dual-Vector Benchmarking
**Context:** Semantic search has been standardized on local embedded LanceDB (`~/.config/one-ship/vectors.lance/`) with BGE-M3 (1024 dimensions, 8,192 token context window, ONNX Runtime) to provide zero-cost, offline-first semantic retrieval with no external network latency or API quota limits (Spec 22 §5).
**Problem:** Google AI Studio offers `gemini-embedding-2` (3,072 dimensions, multimodal) with a free tier limit of 1,000 RPD and 100 RPM. While BGE-M3 provides sufficient accuracy for code and technical documents locally, large enterprise or complex multilingual deployments may benefit from high-dimensional cloud embeddings or dual-vector comparative benchmarking.
**Deferred Solution:**
1. Implement a secondary cloud worker supporting `batchEmbedContents` (up to 100 chunks per request) with priority quota budgeting (preserving 30% of daily quota for interactive queries and 70% for background queue draining).
2. Store dual vector representations in LanceDB (`vector_local: 1024 dims` and `vector_cloud: 3072 dims`).
3. Build an A/B evaluation harness in the daemon to benchmark Top-K retrieval overlap (Jaccard similarity and NDCG) between BGE-M3 and `gemini-embedding-2` on real developer queries before making cloud embeddings a user-configurable option.

## 26. Interactive WebGL 3D CAD Viewport (`Three.js`, Stage A.2)
**Context:** Stage A SolidCraft integration currently renders multi-view headless PNG snapshots (`iso`, `front`, `back`, `top`, `bottom`, `left`, `right`) and section cuts (`--section`) via `solidcraft-cli snapshot` in [ProjectCadTab.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectCadTab.vue).
**Problem:** Static PNG views require switching preset angles and re-rendering on the CLI rather than allowing smooth client-side orbit, rotate, and zoom inspection in 3D space.
**Deferred Solution:**
1. Extend `kopeng::solidcraft` with cached mesh export via `solidcraft-cli mcp` (`export` tool to binary `.stl` or `.obj` inside bounded temp/cache storage).
2. Integrate `three` (`STLLoader` / `OBJLoader` + `OrbitControls`) as an optional interactive WebGL mode alongside the headless PNG snapshot + section-cut viewport.

## 28. Multi-Account Credential Hot-Swapping Viability for Claude Code CLI
**Context:** Unlike Antigravity CLI (`agy`), which reads credentials from the OS keyring on startup, Anthropic Claude Code CLI (`claude`) manages a single active session directly inside `~/.claude/.credentials.json` (Spec 17 §18.1).
**Problem:** Currently `discover_claude_account` imports the active Claude session into SQLite (`provider = 'claude'`), and `switch_account` only toggles `is_active` in SQLite without rewriting `~/.claude/.credentials.json`. It is unverified whether overwriting `~/.claude/.credentials.json` from ONE-SHIP is viable without breaking active Claude Code token refresh cycles or session locks.
**Deferred Solution:** Keep Claude Code in single-account discovery/telemetry mode until multi-account credential file swapping is explicitly needed and tested.

## 30. Pending Electronics Workstation Subsystems & SPICE/EDA Bridges (Spec 31 & Spec 33)
**Context:** The Electronics Lab currently implements the Owon VDS1022 USB oscilloscope driver, `fx2lafw` / `sigrok-cli` logic analyzer capture, MCU toolchain probing (`platformio`, `probe-rs`, `esptool`, `arduino-cli`), and `/sys/class/tty` port discovery.
**Problem:** Several subsystems specified in Spec 31 and Spec 33 §2.3/§3.3 remain unimplemented (with the UI Serial Monitor in `ProjectElectronicsTab.vue` currently acting as a local echo mock).
**Deferred Solution:**
1. **Serial Monitor Backend (Spec 31 §5.3):** Replace the local echo mock in `ProjectElectronicsTab.vue` with a real Rust `serialport`/`tokio-serial` streaming backend.
2. **Legacy Schematic Import Parsers (Spec 31 §6):** Implement EAGLE (`.sch`/`.brd`) and KiCad (`.kicad_sch`) parsers + KiCanvas viewer for importing pre-existing third-party hardware projects.
3. **Hardware Abstraction & MCP Tools (Spec 31 §3, §8):** Unify instruments under `trait HardwareInstrument` and expose the 6 `electronics_*` MCP tools in `src-tauri/src/mcp.rs`.
4. **SPICE / EIS Simulation Bridge (Spec 31 §7, Spec 33 §2.3, §3.3):** Add `ngspice` / `Xyce` netlist extraction and simulation execution (`Run SPICE / EIS Sweep`).

## 32. Configurable Oscilloscope Channel Colors Matching Probe Rings (Spec 31)
**Context:** `OscilloscopeView.vue` currently hardcodes CH1 (`#eab308` yellow) and CH2 (`#06b6d4` cyan) waveform colors.
**Problem:** Hardware oscilloscope probes have colored identification rings that vary by probe set; channel colors should not be coupled to the global UI theme, nor hardcoded without user configuration.
**Deferred Solution:** Expose configurable per-channel color settings (global/electronics config) so the user can match each screen trace color to the physical probe ring color.

## 33. Unified Subtab Styling & Shared Tab-Bar Component (Spec 35, PARTIALLY RESOLVED)
**Context:** Multiple workspace tabs (`Project` / `GithubCollabPanel.vue`, `Journal`, `Electronics`, `Skills`) previously rendered internal subtabs using ad-hoc local styles.
**Status:** `SubtabBar.vue` has been created and adopted across `ProjectManagementTab.vue`, `GithubCollabPanel.vue`, `ProjectSkillsTab.vue`, and `ProjectElectronicsTab.vue` (all wired to `useUiSurfaceNode`). Any remaining ad-hoc subtabs (such as `Journal` if subtabs are added) should use `SubtabBar.vue`.

## 34. Remaining Workspace Panel Voice/Surface Actions (Spec 36)
**Context:** `overview`, `project`, `project/github`, and `files` now expose `UiSurfaceActionDef` handlers in `ui_state.rs` and `voice_controller.rs`.
**Problem:** The remaining workspace subsystems (`devlog`, `git`, `terminal`, `profiler`, `skills`, `electronics`, `design`, `cad`, `pcb`) do not yet expose panel-specific surface actions to the Global Voice Controller.
**Deferred Solution:** Implement and register surface actions panel-by-panel following Spec 35/36.

## 35. Dynamic Voice Model Routing & Cadence Controls (Spec 36)
**Context:** `voice_controller` now supports choosing `gemini-3.8-live` (default) vs `gemini-3.8-live-extended-thinking` and selecting from 30 prebuilt voices in Settings, but model switching is static per session and `gemini-3.8-live` does not expose a numeric `speakingRate` parameter.
**Problem:** Complex multi-step voice requests could benefit from automatic routing between the low-latency model and extended-thinking model, and speaking cadence cannot be tuned numerically without enterprise-allowlisted `replicatedVoiceConfig`.
**Deferred Solution:** Evaluate task-complexity routing once all panel tools are complete, and revisit `replicatedVoiceConfig` if custom cadence samples are required.

## 37. WebKitGTK `backdrop-filter` Variable Bug & Backdrop Layer Leak (Spec 13 §3.9, WORKAROUND ACTIVE)
**Context:** Modal overlays require a standardized scrim plus a background blur (Spec 13 §3.7/§3.9) on `webkit2gtk-4.1 2.54.1-1`.
**Problem:** A prior session concluded that `backdrop-filter` was unsupported on WebKitGTK and attempted to emulate the effect with `filter: blur(...)` over the application DOM tree. That approach re-rasterizes the whole subtree on every invalidation (including modal scrolling) and creates a containing block that breaks `position: fixed` descendants, producing severe scroll jank.
**Root Cause:** Two independent issues. (1) WebKit bug [#289800](https://bugs.webkit.org/show_bug.cgi?id=289800) (`-webkit-backdrop-filter ignores css variables`, status NEW) makes `backdrop-filter` a no-op whenever its value references a CSS custom property. Tailwind CSS v4 builds every `backdrop-blur-*` utility from `var(--tw-backdrop-blur,)` plus eight sibling variables, so all 31 usages were silently inert; literal values (`blur(12px)`) do render correctly. (2) Even with literal values nothing rendered because `scripts/run_dev.sh` launched the GUI with `WEBKIT_DISABLE_DMABUF_RENDERER=1`. In WebKitGTK 2.54.1 `AcceleratedBackingStore::rendererBufferTransportMode()` returns an empty transport set for any truthy value of that variable, so `canUseHardwareAcceleration()` returns false and the web process runs without accelerated compositing. `backdrop-filter` is a compositor-only effect and is ignored by the software path.
**Current Workaround:** `.ui-overlay-backdrop` in `src/styles/ui.css` declares `-webkit-backdrop-filter: blur(12px)` / `backdrop-filter: blur(12px)` with literal values, all 31 modal/blocking overlays use it, and `scripts/run_dev.sh` no longer sets `WEBKIT_DISABLE_DMABUF_RENDERER` (use `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` if the DMA-BUF path ever regresses). `filter: blur()` on the application tree and Tailwind `backdrop-*` utilities are prohibited app-wide (Spec 13 §3.9).
**Remaining Risk:** WebKitGTK 2.54 has a separate layer leak ([#328065](https://bugs.webkit.org/show_bug.cgi?id=328065)) that grows with the number of simultaneous `backdrop-filter` layers (stable at ~10-14 layers, multi-gigabyte growth from ~18-22). The overlay contract therefore caps modal nesting at two transient levels and forbids per-row/per-HUD blurred backdrops.
**Revisit When:** WebKit #289800 is resolved and/or the runtime moves past WebKitGTK 2.54; then re-evaluate allowing Tailwind backdrop utilities, multiple simultaneous blurred surfaces, and per-HUD glass effects.
**Packaging Follow-up:** Spec 12 §7.4 defines the global-only `[system] webkit_renderer_mode` setting (`auto|dmabuf|shared_memory|software`), the `auto` ladder, and the restart-scoped resolution in `main()`. Installers may seed the initial value from GPU/driver detection, but the app must resolve it at runtime; `shared_memory` is the NVIDIA fallback that preserves compositing and blur, and `software` is the last resort.

## 45. Legacy `bg-black/*` Overlay & Panel Surface Migration (Spec 13 §1.2 / §11.2)
**Context:** Post-audit review (`2026-11-19`) migrated `OscilloscopeView.vue` and `ProjectElectronicsTab.vue` to `--ui-bg-*` tokens and `QuotaGaugePreviewSelector.vue` / `AppExecutionConfigPicker.vue` off `text-[7px]`, leaving ~90 legacy `bg-black/*` and `hover:bg-white/5` classes across ~19 older modals and workspace panels (`ProjectWorkspace.vue`, `TaskReportModal.vue`, `ProjectInitModal.vue`, `AddPersonaModal.vue`, `ProjectReaTab.vue`, `DocstyleSelectionModal.vue`, `ApprovalRequestModal.vue`, `ScriptLaunchModal.vue`, `ConfirmDeleteModal.vue`, `RollingStatusLog.vue`) plus 7 remaining `text-[7px]` occurrences (`ProjectWorkspace.vue`: 4, `ProjectTerminalGrid.vue`: 3).
**Deferred Solution:** Migrate remaining `bg-black/*`, `hover:bg-white/5`, and `text-[7px]` occurrences panel-by-panel to Spec 13 `--ui-bg-*` tokens and `>= 8px` typography (keeping only `FloatingChatOverlay.vue`'s Live Terminal Stream `bg-black` exception) until `./scripts/verify_rules.sh` passes with zero failures.

## 46. Semantic Audit REST Endpoint, Legacy DB Column Suffixes & Monolithic SFC Decomposition
**Context:** Post-audit review (`2026-11-19`) identified structural items deferred from quick remediation:
1. `POST /api/v1/audit/semantic` in `mcp.rs` sits outside `api::API_COMMANDS` / `daemonApi.ts` (`AGENTS.md` §9.6).
2. Legacy SQLite columns (`cost_dollars`, `total_tokens`, `context_pct`) in `agent_session_journal` predate Tiger Style `_count`/`_percent` unit suffixes.
3. Internal `chunker.rs` / `store.rs` heuristics and debounce floor clamps remain module constants, and `ProjectWorkspace.vue` (2637 LOC), `ProjectManagementTab.vue` (1773 LOC), and `ProjectConfigPanel.vue` (1023 LOC) exceed ~500 LOC.
4. `AppHeader.vue:222-225` retains canvas fallback hex constants (`#22c55e`, `#f59e0b`, `#eab308`) when CSS variables are queried before theme load.
**Deferred Solution:** Address in a dedicated architectural cleanup cycle (converting `/api/v1/audit/semantic` to a daemon command or documented MCP-only route, splitting large SFCs into composables/subcomponents, and migrating DB column names with a clean schema version).

## 47. Light Theme Text-vs-Background Contrast & Surface Mix Tuning (Spec 13)
**Context:** Built-in light themes (`Catppuccin Latte`, `Tokyo Night Day`, `Gruvbox Light`, `Rose Pine Dawn`, `Solarized Light`) were added alongside dark presets in [theme.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/theme.rs) and `App.vue` (`data-theme-mode="light"`).
**Problem:** Several UI surfaces and components exhibit low text-vs-background contrast in light mode—primarily due to `ui.css` mixing `--ui-bg-title` / `--ui-bg-list` with `#000`, hardcoded `bg-black/*` overlays (see Debt #45), and `dim_color` / `fg_color` pairings on tinted or recessed surfaces.
**Deferred Solution:** Audit all panels and modals under each light preset, adjust `[data-theme-mode="light"]` surface `color-mix()` rules in [ui.css](file:///home/butcherwutcher/Projects/dev/one-ship/src/styles/ui.css) (e.g., mixing with `#fff` or `--fg-color` where appropriate), and tune light preset tokens / component contrast so all text meets readable contrast ratios.

## 48. Omarchy Bar Widget: Production Telemetry vs. Dev Launcher Separation (Spec 41 Phase 4)
**Context:** `integrations/omarchy-bar/` (`tech.kirulab.one-ship`) currently wraps `scripts/run_dev.sh`, displaying `"Development"`, Cargo build progress bars, `oneship-dev-vite`, `watchexec`, and `ghostty` journal viewers.
**Problem:** End users installing ONE-SHIP in normal mode do not have `run_dev.sh`, Vite, or Cargo watchers, and Spec 12 §7.1 prevents the bar widget from querying `/api/*` directly.
**Deferred Solution:** Separate the developer unit launcher (`tech.kirulab.one-ship-dev`) from a user-facing `tech.kirulab.one-ship` Omarchy widget that manages `oneship-daemon.service` / `one-ship` and reads a lightweight read-only status summary (`$XDG_RUNTIME_DIR/oneship-status.json` or restricted Unix socket) to show active/working agent counts, pending approvals, and running services.

## 49. Host Capabilities Registry, Dynamic UI Gating & OpenDictate Unavailability Handling (Spec 41 Phase 3)
**Context:** `get_all_plugin_schemas()` (`config.rs`), `SettingsModal.vue`, `ProjectConfigPanel.vue`, and `ProjectWorkspace.vue` expose all optional domain plugins (`rea`, `desigma`, `solidcraft`, `pcbcraft`, `electronics`) and all runner options regardless of what binaries exist on the host. Additionally, `voiceStore.ts` treats OpenDictate unavailability (`gdbus` failure) as `"busy"` and polls every 1.2s, while `opendictate.rs` loops `gdbus monitor` every 5s when `gdbus` is absent and reads `/tmp/opendictate_state.json`.
**Problem:** Users who install only core ONE-SHIP (or omit optional toolchains like PCBCraft/REA/OpenDictate) see broken tabs, irrelevant settings categories, and background polling/log noise.
**Deferred Solution:** Implement `get_host_capabilities` (`HostCapabilities`), gate workspace tabs, plugin toggles, Settings categories, and runner selectors based on real host availability (plus an *Integrations* panel for custom path overrides), and separate `Unavailable` from `Busy` in OpenDictate state handling.
