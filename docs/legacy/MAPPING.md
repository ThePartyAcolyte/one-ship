# Legacy-to-Current Specification Mapping

Complete traceability matrix from the original 39 numbered specification files (`docs/legacy/specs/`) to the 41 reorganized living specifications (`docs/specs/`).

> [!NOTE]
> Numbers `15`, `27`, and `28` did not exist in the legacy series, and `26` was previously duplicated (`26_greenfield_project_creation_and_conversational_bootstrapping.md` and `26_rea_reverse_engineering_integration.md`). Every requirement, schema, command, event, and configuration key from all 39 legacy files has been preserved in the reorganized specs below.

| Legacy File (`docs/legacy/specs/`) | Current Specification(s) (`docs/specs/`) | Notes |
| :--- | :--- | :--- |
| `01_service_orchestrator.md` | [03-projects/01-project-registry-and-service-orchestration.md](../specs/03-projects/01-project-registry-and-service-orchestration.md) | 1:1 rewrite |
| `02_diagnostic_laboratory.md` | [07-ui/07-diagnostic-laboratory-and-tray-hub.md](../specs/07-ui/07-diagnostic-laboratory-and-tray-hub.md) | Consolidated deferred post-`0.2.x` roadmap specs |
| `03_tray_hub_interface.md` | [07-ui/07-diagnostic-laboratory-and-tray-hub.md](../specs/07-ui/07-diagnostic-laboratory-and-tray-hub.md) | Consolidated deferred post-`0.2.x` roadmap specs |
| `04_dictation_and_nlp.md` | [05-voice/02-dictation-and-agent-chat.md](../specs/05-voice/02-dictation-and-agent-chat.md) | 1:1 rewrite |
| `05_dictate_ipc_protocol.md` | [05-voice/02-dictation-and-agent-chat.md](../specs/05-voice/02-dictation-and-agent-chat.md) | Replaced by native Gemini Live dictation |
| `06_plugin_architecture.md` | [01-architecture/01-plugin-architecture.md](../specs/01-architecture/01-plugin-architecture.md) | 1:1 rewrite |
| `07_i18n_translation_strategy.md` | [07-ui/06-localization-and-belta-language.md](../specs/07-ui/06-localization-and-belta-language.md) | Merged with `14_cinturonero_translator_protocol.md` |
| `08_native_process_supervisor.md` | [02-agents/01-native-process-supervisor.md](../specs/02-agents/01-native-process-supervisor.md) | 1:1 rewrite |
| `09_vcs_and_logging_architecture.md` | [03-projects/02-git-workspace-and-branches.md](../specs/03-projects/02-git-workspace-and-branches.md)<br>[03-projects/03-system-logging-and-status-terminal.md](../specs/03-projects/03-system-logging-and-status-terminal.md) | Split cleanly into Git workspace vs. system logging |
| `10_configuration_schema_and_cascade.md` | [01-architecture/02-configuration-schema-and-cascade.md](../specs/01-architecture/02-configuration-schema-and-cascade.md)<br>[01-architecture/03-platform-skill-injection.md](../specs/01-architecture/03-platform-skill-injection.md) | Split cascade/schemas vs. platform skill injection |
| `11_remote_github_integration.md` | [03-projects/04-remote-repositories-and-diagnostics.md](../specs/03-projects/04-remote-repositories-and-diagnostics.md) | Merged with remote registry from `39` |
| `12_streamable_http_mcp_daemon.md` | [01-architecture/04-streamable-http-mcp-daemon.md](../specs/01-architecture/04-streamable-http-mcp-daemon.md)<br>[01-architecture/05-daemon-operator-api-and-lifecycle.md](../specs/01-architecture/05-daemon-operator-api-and-lifecycle.md) | Split MCP HTTP server (§1–§6) vs. operator RPC & daemon lifecycle (§7) |
| `13_design_system_and_ui_components.md` | [07-ui/01-design-system-foundations.md](../specs/07-ui/01-design-system-foundations.md)<br>[07-ui/02-ui-component-library-and-recipes.md](../specs/07-ui/02-ui-component-library-and-recipes.md)<br>[07-ui/03-workspace-chrome-and-panel-contracts.md](../specs/07-ui/03-workspace-chrome-and-panel-contracts.md) | Split into foundations, component library, and chrome/panel contracts |
| `14_cinturonero_translator_protocol.md` | [07-ui/06-localization-and-belta-language.md](../specs/07-ui/06-localization-and-belta-language.md) | Merged with `07_i18n_translation_strategy.md` |
| `16_agent_session_snapshot_system.md` | [02-agents/06-agent-session-snapshots.md](../specs/02-agents/06-agent-session-snapshots.md) | 1:1 rewrite |
| `17_multi_account_management.md` | [02-agents/04-multi-account-management.md](../specs/02-agents/04-multi-account-management.md) | 1:1 rewrite |
| `18_mcp_dialogue_channel_and_chat_overlay.md` | [02-agents/03-mcp-dialogue-and-chat-overlay.md](../specs/02-agents/03-mcp-dialogue-and-chat-overlay.md) | 1:1 rewrite (condensed obsolete devlog checklists) |
| `19_agent_quota_and_context_observability.md` | [02-agents/05-quota-observability-and-token-economy.md](../specs/02-agents/05-quota-observability-and-token-economy.md) | Merged with `23_token_economy_and_quota_kickstart.md` |
| `20_live_artifacts_and_project_scratchpad.md` | [03-projects/08-live-artifacts-and-scratchpad.md](../specs/03-projects/08-live-artifacts-and-scratchpad.md) | 1:1 rewrite |
| `21_agent_pixel_portrait_dna.md` | [02-agents/08-agent-pixel-portrait-dna.md](../specs/02-agents/08-agent-pixel-portrait-dna.md) | 1:1 rewrite |
| `22_session_lifecycle_and_devlog_graph.md` | [04-observability/01-session-lifecycle-and-devlog-graph.md](../specs/04-observability/01-session-lifecycle-and-devlog-graph.md) | 1:1 rewrite |
| `23_token_economy_and_quota_kickstart.md` | [02-agents/05-quota-observability-and-token-economy.md](../specs/02-agents/05-quota-observability-and-token-economy.md) | Merged with `19_agent_quota_and_context_observability.md` |
| `24_project_versioning_and_migration_engine.md` | [01-architecture/06-project-versioning-and-migration.md](../specs/01-architecture/06-project-versioning-and-migration.md) | 1:1 rewrite |
| `25_universal_telemetry_and_profiling_engine.md` | [04-observability/02-universal-telemetry-and-profiling.md](../specs/04-observability/02-universal-telemetry-and-profiling.md) | 1:1 rewrite |
| `26_greenfield_project_creation_and_conversational_bootstrapping.md` | [03-projects/10-greenfield-project-creation.md](../specs/03-projects/10-greenfield-project-creation.md) | 1:1 rewrite (resolved duplicate `26_` prefix) |
| `26_rea_reverse_engineering_integration.md` | [06-hardware-and-cad/04-rea-reverse-engineering-integration.md](../specs/06-hardware-and-cad/04-rea-reverse-engineering-integration.md) | 1:1 rewrite (resolved duplicate `26_` prefix) |
| `29_on_demand_skill_manager_and_mcp_protocol.md` | [02-agents/07-on-demand-skills.md](../specs/02-agents/07-on-demand-skills.md) | 1:1 rewrite |
| `30_agent_thread_conversation_and_context_continuity.md` | [02-agents/02-thread-conversation-and-continuity.md](../specs/02-agents/02-thread-conversation-and-continuity.md) | 1:1 rewrite |
| `31_electronics_workstation_and_instrumentation_engine.md` | [06-hardware-and-cad/01-electronics-workstation-and-instrumentation.md](../specs/06-hardware-and-cad/01-electronics-workstation-and-instrumentation.md) | 1:1 rewrite |
| `32_oneship_designer_integration.md` | [06-hardware-and-cad/02-oneship-designer-integration.md](../specs/06-hardware-and-cad/02-oneship-designer-integration.md) | 1:1 rewrite |
| `33_cad_and_pcb_integration.md` | [06-hardware-and-cad/03-cad-and-pcb-integration.md](../specs/06-hardware-and-cad/03-cad-and-pcb-integration.md) | 1:1 rewrite |
| `34_project_management_and_oss_collaboration.md` | [03-projects/07-project-management-and-milestones.md](../specs/03-projects/07-project-management-and-milestones.md) | 1:1 rewrite |
| `35_ui_state_machine_and_session_persistence.md` | [07-ui/04-ui-state-machine-and-session-persistence.md](../specs/07-ui/04-ui-state-machine-and-session-persistence.md) | 1:1 rewrite |
| `36_global_voice_controller.md` | [05-voice/03-global-voice-controller.md](../specs/05-voice/03-global-voice-controller.md) | 1:1 rewrite |
| `37_project_settings_tab_and_overview_dashboard.md` | [07-ui/05-project-settings-and-overview-dashboard.md](../specs/07-ui/05-project-settings-and-overview-dashboard.md) | 1:1 rewrite |
| `38_github_inbox_and_triage_flags.md` | [03-projects/06-forge-inbox-and-triage.md](../specs/03-projects/06-forge-inbox-and-triage.md) | 1:1 rewrite |
| `39_forge_provider_and_project_surface_handoff.md` | [03-projects/04-remote-repositories-and-diagnostics.md](../specs/03-projects/04-remote-repositories-and-diagnostics.md)<br>[03-projects/05-forge-provider-abstraction.md](../specs/03-projects/05-forge-provider-abstraction.md) | Remote registry merged into `04`; Forge provider & project surfaces in `05` |
| `40_unified_capability_core_and_voice_harness.md` | [05-voice/04-unified-capability-core-and-voice-harness.md](../specs/05-voice/04-unified-capability-core-and-voice-harness.md) | 1:1 rewrite |
| `41_linux_packaging_and_host_capabilities.md` | [01-architecture/07-linux-packaging-and-host-capabilities.md](../specs/01-architecture/07-linux-packaging-and-host-capabilities.md)<br>[01-architecture/03-platform-skill-injection.md](../specs/01-architecture/03-platform-skill-injection.md) | Host capabilities & packaging in `07`; relocatable templates root also in `03` |
