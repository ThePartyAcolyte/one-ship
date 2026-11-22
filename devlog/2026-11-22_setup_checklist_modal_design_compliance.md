# 2026-11-22 — Setup & Readiness Modal Design System Compliance

## Summary
- Audited and updated `SetupChecklistModal.vue` to comply with `docs/specs/07-ui/01-design-system-foundations.md` and `docs/specs/07-ui/02-ui-component-library-and-recipes.md`.
- Replaced the ad-hoc `bg-[var(--bg-color)]/80` overlay scrim with the standard `.ui-overlay-backdrop` class (`rgba(0, 0, 0, 0.45)` + `backdrop-filter: blur(12px)`).
- Standardized modal container surface to `ui-surface-base` and modal header/footer surfaces to `ui-surface-title`.
- Wrapped the icon-only modal close button in `<AppTooltip text="Close">`.
- Standardized inline action buttons (`h-6 px-2 text-[9px] transition-colors`) and form action buttons (`h-7` / `h-8`, `text-[9px]`, `transition-colors`), reserving solid accent fill for the `Done` footer action and using `bg-[var(--accent-color)]/15` for `Create & Save`.
- Replaced the hardcoded `/home/user/Projects` placeholder with `~/Projects`.

## Files Modified / Created
- [SetupChecklistModal.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/SetupChecklistModal.vue)

## Verification

### 1. Targeted Rule Check (`rg -n`)
```text
rg -n ":\s*any\b|\btitle=|zinc-|emerald-|amber-|rose-|cyan-|slate-|text-white|bg-black/|bg-\[#|backdrop-blur|/home/|catch\s*(\([^)]*\))?\s*\{\s*\}" src/components/modals/SetupChecklistModal.vue || echo "NO_VIOLATIONS"
NO_VIOLATIONS
```

### 2. Daemon Health (`./scripts/run_dev.sh status`)
```json
{"units":{"daemon":{"active_state":"active","sub_state":"running","result":"success","main_pid":1305472,"log_tail":null},"daemon_build":{"active_state":"inactive","sub_state":"dead","result":"success","main_pid":null,"log_tail":null},"daemon_watch":{"active_state":"active","sub_state":"running","result":"success","main_pid":335401,"log_tail":null},"vite":{"active_state":"active","sub_state":"running","result":"success","main_pid":332395,"log_tail":null},"gui":{"active_state":"inactive","sub_state":"dead","result":"success","main_pid":null,"log_tail":null}},"daemon_listening_ports":[],"vite_listening_ports":[],"gui_window_pid":null,"build_progress":{"phase":"cargo_build","completed_count":1204,"total_count":1205,"percent":99,"current_item":"linking","started_at_seconds":1795381215,"last_duration_seconds":null},"gui_progress":{"phase":"idle","completed_count":null,"total_count":null,"percent":null,"current_item":null,"started_at_seconds":1795381215,"last_duration_seconds":86}}
```
