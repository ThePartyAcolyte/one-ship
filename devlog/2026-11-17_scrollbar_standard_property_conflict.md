# 2026-11-17 — Scrollbar Unification: Tailwind `scrollbar-thin` vs WebKit Custom Scrollbars

## Summary

Resolved Technical Debt #36. The global `::-webkit-scrollbar` contract in `src/styles/ui.css` (constant `6px`, square, dark track, accent thumb on hover) was being selectively ignored by WebKitGTK 2.54. The culprit was the Tailwind CSS v4 built-in `scrollbar-thin` utility, which emits the standard `scrollbar-width: thin` property and permanently disables WebKit's custom scrollbar renderer on every element carrying it.

## Root Cause

WebKit chooses the custom scrollbar implementation per scrollable area in `RenderLayerScrollableArea::createScrollbar()`:

```cpp
bool usesLegacyScrollbarStyle = actualRenderBox && actualRenderBox->style().usesLegacyScrollbarStyle();
if (usesLegacyScrollbarStyle && element)
    widget = RenderScrollbar::createCustomScrollbar(*this, orientation, element.get());
else
    widget = Scrollbar::createNativeScrollbar(*this, orientation, scrollbarWidthStyle());
```

`usesLegacyScrollbarStyle()` is defined in `Source/WebCore/style/computed/StyleComputedStyle+GettersInlines.h`:

```cpp
// ignore non-standard ::-webkit-scrollbar when standard properties are in use
inline bool ComputedStyle::usesStandardScrollbarStyle() const
{
    return scrollbarWidth() != Style::ScrollbarWidth::Auto || !scrollbarColor().isAuto();
}

inline bool ComputedStyle::usesLegacyScrollbarStyle() const
{
    return hasPseudoStyle(PseudoElementType::WebKitScrollbar) && !usesStandardScrollbarStyle();
}
```

Any element whose computed `scrollbar-width` or `scrollbar-color` is not `auto` never receives a custom `RenderScrollbar`; all `::-webkit-scrollbar` declarations are discarded, including `!important`.

Tailwind CSS v4.3.3 registers the utility as `staticUtility('scrollbar-thin', [['scrollbar-width', 'thin']])`. The codebase carried 31 `scrollbar-thin` usages across 16 components, so every one of those scroll containers fell back to the native Adwaita scrollbar. `SettingsModal.vue` was one of the few large panels without the class, which is why it appeared to be the only place where the contract worked.

`GTK_OVERLAY_SCROLLING=0` and `color-scheme: dark` could not fix this because they only configure the native fallback. `ScrollbarThemeAdwaita::scrollbarThickness()` ignores `ScrollbarWidth::Thin` and always returns the constant `scrollbarSize`, so `scrollbar-thin` never produced a thin native bar — it only removed the thin custom one.

## Changes Implemented

- **Removed all 31 `scrollbar-thin` class tokens** from `ProjectWorkspace.vue` (4), `AppExecutionConfigPicker.vue` (3), `DevlogJournalView.vue` (3), `FloatingChatOverlay.vue` (3), `ProjectGitSnapshotsTab.vue` (3), `ProjectProfilerTab.vue` (3), `ProjectSkillsTab.vue` (2), `ProjectTerminalGrid.vue` (2), `RollingStatusLog.vue` (1), `ScriptLaunchModal.vue` (1), `ApprovalRequestModal.vue` (1), `AddPersonaModal.vue` (1), `AppFileTree.vue` (1), `ProjectVerticalRail.vue` (1), `ProjectOverviewTab.vue` (1), and `AgentTaskbar.vue` (1). With `scrollbar-width` back to `auto`, every container now uses the shared `::-webkit-scrollbar` contract.
- **`docs/specs/13_design_system_and_ui_components.md` §3.8:** Renamed the section from "Target Design" to the active contract, declared `src/styles/ui.css` as the single source of truth, documented the WebKit `usesLegacyScrollbarStyle()` gate, and prohibited `scrollbar-thin` / `scrollbar-thumb-*` / `scrollbar-track-*` on scroll containers. Noted that `scrollbar-color` is inherited, so an ancestor-level standard utility would break every nested scroll container.
- **`TECHNICAL_DEBT.md` #36:** Marked RESOLVED with the root cause and resolution. Also relocated a stray `Deferred Solution` line that belonged to item #35.

## Verification

- `grep -rn "scrollbar-thin" src` returns no matches.
- No build, daemon restart, or dev-server command was executed by the agent (AGENTS.md §1.6); the self-managed watcher performs the frontend rebuild.
- Manual confirmation pending: system log viewer, Git `Files Changed`, floating chat, Overview, Profiler, Terminal, Skills, taskbar dropdown, and the Settings modal should all render identical `6px` square scrollbars with an accent thumb on hover.

## Anti-Regression Rule

Scrollbar behavior must only be changed through `src/styles/ui.css`. Standard CSS Scrollbars Level 1 properties (`scrollbar-width`, `scrollbar-color`) intentionally disable the custom renderer in WebKit and must not be used, except `scrollbar-width: none` inside `.scrollbar-none` for deliberately invisible bars.
