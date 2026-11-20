<script setup lang="ts">
import { ref, shallowRef, watch, computed, onMounted, onUnmounted } from 'vue';
import { Settings, X, Search, RefreshCw, Check, AlertTriangle } from '@lucide/vue';
import { AppTooltip, AppSelect, AppInput, AppToggle, QuotaGaugePreviewSelector } from '../ui';
import type { SelectOption } from '../ui';
import { useAgentMetricsStore, type QuotaGaugeStyle } from '../../stores/agentMetricsStore';
import { useCoreStore } from '../../stores/coreStore';
import { useRunnerCatalog } from '../../utils/runnerCatalog';
import { INTEGRATION_INSTALL_GUIDES } from '../../utils/integrationGuides';
import { daemonCall } from '../../utils/daemonClient';
import { emitUiEvent } from '../../utils/uiEvents';
import type { ConfigFieldSchema, PluginConfigSchema, ProjectRecord } from '../../types';
import type { AgentPairSchema } from '../../types/generated/AgentPairSchema';
import type { JsonValue } from '../../types/generated/serde_json/JsonValue';

type ConfigPrimitive = string | number | boolean | string[];

const coreStore = useCoreStore();
const runnerCatalog = useRunnerCatalog();
const isRefreshingCapabilities = ref(false);

/** Field types whose options come from the runner manifests (`list_agent_runners`). */
const RUNNER_FIELD_TYPES = ['runner', 'runner_model', 'runner_effort'];

const props = defineProps<{
  show: boolean;
  scope: 'global' | 'project';
  project: ProjectRecord | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'saved', scope: 'global' | 'project', name?: string): void;
}>();

// Replaced as a whole on load; `shallowRef` also skips unwrapping the recursive `JsonValue` defaults.
const pluginSchemas = shallowRef<PluginConfigSchema[]>([]);
const searchQuery = ref<string>("");
const activeSettingsTab = ref<string>("");
// eslint-disable-next-line @typescript-eslint/no-explicit-line
const settingsValues = ref<Record<string, Record<string, ConfigPrimitive>>>({});
const settingsProvenance = ref<Record<string, string>>({});
const rawGlobalConfig = shallowRef<JsonValue | null>(null);
const isSavingSettings = ref(false);
const settingsError = ref<string | null>(null);
const isTestingVoice = ref(false);

/** Closes the modal on Escape; runs in the capture phase so it wins over nested controls. */
function handleGlobalKeydown(e: KeyboardEvent) {
  if (!props.show || e.key !== 'Escape') return;
  e.preventDefault();
  e.stopImmediatePropagation();
  emit('close');
}

onMounted(() => window.addEventListener('keydown', handleGlobalKeydown, { capture: true }));
onUnmounted(() => window.removeEventListener('keydown', handleGlobalKeydown, { capture: true }));

async function testSelectedVoice() {
  if (isTestingVoice.value) return;
  const vc = settingsValues.value['voice_controller'] ?? {};
  const voiceName = String(vc['voice_name'] ?? 'Fenrir');
  const model = String(vc['model'] ?? 'gemini-3.8-live');
  const personaStyle = String(vc['persona_style'] ?? 'belta');
  isTestingVoice.value = true;
  settingsError.value = null;
  try {
    await daemonCall('test_voice_controller_voice', {
      voiceName,
      model,
      personaStyle,
    });
  } catch (err: unknown) {
    settingsError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isTestingVoice.value = false;
  }
}

// Filter plugin schemas and their fields by scope, host capability availability, and search query
const filteredPluginSchemas = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();

  const mapped = pluginSchemas.value
    .filter(plugin => coreStore.isIntegrationAvailable(plugin.plugin_id))
    .map(plugin => {
      const scopedFields = plugin.fields.filter(f =>
        props.scope === 'global' ? f.scope !== 'project_only' : f.scope !== 'global_only'
      );

      if (!query) {
        return {
          ...plugin,
          matchingFields: scopedFields,
          matchCount: scopedFields.length,
        };
      }

      const pluginMatches = plugin.plugin_name.toLowerCase().includes(query) || plugin.plugin_id.toLowerCase();
      const matchingFields = scopedFields.filter(f => 
        pluginMatches ||
        f.label.toLowerCase().includes(query) ||
        f.key.toLowerCase().includes(query) ||
        f.description.toLowerCase().includes(query)
      );

      return {
        ...plugin,
        matchingFields,
        matchCount: matchingFields.length,
      };
    })
    .filter(plugin => plugin.matchingFields.length > 0);

  if (props.scope === 'global' && (!query || 'integrations host capabilities binaries'.includes(query))) {
    mapped.push({
      plugin_id: 'integrations',
      plugin_name: 'Integrations',
      fields: [],
      matchingFields: [],
      matchCount: 0,
      agent_pairs: [],
    });
  }

  return mapped;
});

async function refreshHostCapabilities() {
  if (isRefreshingCapabilities.value) return;
  isRefreshingCapabilities.value = true;
  try {
    await coreStore.fetchHostCapabilities();
  } finally {
    isRefreshingCapabilities.value = false;
  }
}

const INTEGRATION_PATH_FIELDS: Record<string, string[]> = {
  solidcraft: ['app_binary_path', 'cli_binary_path'],
  pcbcraft: ['app_binary_path', 'cli_binary_path'],
  desigma: ['app_binary_path', 'mcp_binary_path'],
  rea: ['ghidra_install_dir'],
};

function editableFieldsForIntegration(pluginId: string): ConfigFieldSchema[] {
  const keys = INTEGRATION_PATH_FIELDS[pluginId];
  if (!keys) return [];
  const schema = pluginSchemas.value.find(p => p.plugin_id === pluginId);
  if (!schema) return [];
  return schema.fields.filter(f => keys.includes(f.key));
}

const totalMatchCount = computed(() => {
  return filteredPluginSchemas.value.reduce((acc, p) => acc + p.matchingFields.length, 0);
});

watch(searchQuery, () => {
  const available = filteredPluginSchemas.value;
  if (available.length > 0 && !available.some(p => p.plugin_id === activeSettingsTab.value)) {
    activeSettingsTab.value = available[0].plugin_id;
  }
});

function formatTomlValue(val: ConfigPrimitive | JsonValue | undefined): string {
  if (typeof val === "boolean") return val ? "true" : "false";
  if (typeof val === "number") return isNaN(val) ? "0" : String(val);
  if (Array.isArray(val)) return `[${val.map(v => formatTomlValue(v as ConfigPrimitive)).join(", ")}]`;
  return JSON.stringify(String(val ?? ""));
}

interface UpdatedConfigPayload {
  system: Record<string, ConfigPrimitive>;
  plugins: Record<string, Record<string, ConfigPrimitive>>;
}

function generateToml(configObj: UpdatedConfigPayload): string {
  const lines: string[] = [];

  if (configObj.system && Object.keys(configObj.system).length > 0) {
    lines.push("[system]");
    for (const [k, v] of Object.entries(configObj.system)) {
      if (typeof v !== "object" || Array.isArray(v)) {
        lines.push(`${k} = ${formatTomlValue(v)}`);
      }
    }
    lines.push("");
  }

  if (configObj.plugins) {
    for (const [pluginId, pluginTable] of Object.entries(configObj.plugins)) {
      if (pluginTable && Object.keys(pluginTable).length > 0) {
        lines.push(`[plugins.${pluginId}]`);
        for (const [k, v] of Object.entries(pluginTable)) {
          if (typeof v !== "object" || Array.isArray(v)) {
            lines.push(`${k} = ${formatTomlValue(v)}`);
          }
        }
        lines.push("");
      }
    }
  }

  return lines.join("\n").trimEnd() + "\n";
}

watch(() => props.show, async (newVal) => {
  if (newVal) {
    searchQuery.value = "";
    await loadSettings();
  }
});

async function loadSettings() {
  settingsError.value = null;
  settingsProvenance.value = {};

  try {
    // Runner, model and effort options come only from the runner manifests.
    await runnerCatalog.loadRunnerCatalog();
    pluginSchemas.value = await daemonCall("get_plugin_schemas", {});

    const values: Record<string, Record<string, ConfigPrimitive>> = {};

    if (props.scope === "project" && props.project) {
      const scopedDetails = await daemonCall("get_scoped_project_config", {
        projectPath: props.project.path,
      });

      for (const plugin of pluginSchemas.value) {
        values[plugin.plugin_id] = {};
        for (const field of plugin.fields) {
          const fullKey = `${plugin.plugin_id}.${field.key}`;
          const detail = scopedDetails[fullKey];
          if (detail) {
            values[plugin.plugin_id][field.key] = detail.value as ConfigPrimitive;
            settingsProvenance.value[fullKey] = detail.provenance;
          } else {
            values[plugin.plugin_id][field.key] = field.default_value as ConfigPrimitive;
            settingsProvenance.value[fullKey] = "default";
          }
        }
      }
    } else {
      const globalCfg = await daemonCall("get_global_config", {});
      rawGlobalConfig.value = globalCfg;
      const cfgObj = (globalCfg && typeof globalCfg === 'object' && !Array.isArray(globalCfg))
        ? globalCfg
        : {};
      const sysRecord = (cfgObj.system && typeof cfgObj.system === 'object' && !Array.isArray(cfgObj.system)
        ? cfgObj.system
        : {}) as Record<string, ConfigPrimitive | undefined>;
      const pluginsMap = (cfgObj.plugins && typeof cfgObj.plugins === 'object' && !Array.isArray(cfgObj.plugins)
        ? cfgObj.plugins
        : {}) as Record<
        string,
        Record<string, ConfigPrimitive | undefined> | undefined
      >;

      for (const plugin of pluginSchemas.value) {
        values[plugin.plugin_id] = {};
        const pluginData: Record<string, ConfigPrimitive | undefined> =
          plugin.plugin_id === "system"
            ? sysRecord
            : (pluginsMap[plugin.plugin_id] ?? {});

        for (const field of plugin.fields) {
          const fullKey = `${plugin.plugin_id}.${field.key}`;
          if (pluginData[field.key] !== undefined) {
            values[plugin.plugin_id][field.key] = pluginData[field.key] as ConfigPrimitive;
            settingsProvenance.value[fullKey] = "global";
          } else if (plugin.plugin_id === "appearance" && field.key === "body_font" && typeof localStorage !== 'undefined' && localStorage.getItem('oneship-body-font')) {
            values[plugin.plugin_id][field.key] = localStorage.getItem('oneship-body-font') || 'sans';
            settingsProvenance.value[fullKey] = "global";
          } else if (plugin.plugin_id === "appearance" && field.key === "font_scale" && typeof localStorage !== 'undefined' && localStorage.getItem('oneship-font-scale')) {
            values[plugin.plugin_id][field.key] = localStorage.getItem('oneship-font-scale') || '100';
            settingsProvenance.value[fullKey] = "global";
          } else if (plugin.plugin_id === "appearance" && field.key === "quota_gauge_style" && typeof localStorage !== 'undefined' && localStorage.getItem('oneship_quota_gauge_style')) {
            values[plugin.plugin_id][field.key] = localStorage.getItem('oneship_quota_gauge_style') || 'concentric';
            settingsProvenance.value[fullKey] = "global";
          } else {
            values[plugin.plugin_id][field.key] = field.default_value as ConfigPrimitive;
            settingsProvenance.value[fullKey] = "default";
          }
        }
      }
    }

    settingsValues.value = values;

    const available = filteredPluginSchemas.value;
    if (available.length > 0 && !available.some(p => p.plugin_id === activeSettingsTab.value)) {
      activeSettingsTab.value = available[0].plugin_id;
    }
  } catch (e: unknown) {
    console.error("Failed to load settings:", e);
    settingsError.value = e instanceof Error ? e.message : String(e);
  }
}

async function saveSettings() {
  if (pairProblems.value.length > 0) {
    settingsError.value = pairProblems.value.join('; ');
    return;
  }
  isSavingSettings.value = true;
  settingsError.value = null;

  try {
    const updated: UpdatedConfigPayload = { system: {}, plugins: {} };

    for (const plugin of pluginSchemas.value) {
      const targetTable = plugin.plugin_id === "system"
        ? updated.system
        : (updated.plugins[plugin.plugin_id] = updated.plugins[plugin.plugin_id] || {});

      for (const field of plugin.fields) {
        if (props.scope === "global" && field.scope === "project_only") continue;
        if (props.scope === "project" && field.scope === "global_only") continue;
        const val = settingsValues.value[plugin.plugin_id]?.[field.key];
        targetTable[field.key] = val !== undefined ? val : (field.default_value as ConfigPrimitive);
      }
    }

    const tomlContent = generateToml(updated);

    if (props.scope === "project" && props.project) {
      await daemonCall("save_project_rula_config", {
        projectPath: props.project.path,
        configToml: tomlContent,
      });
      emit('saved', 'project', props.project.name);
    } else {
      await daemonCall("save_global_config", { configToml: tomlContent });
      await coreStore.fetchHostCapabilities();
      emit('saved', 'global');
    }

    if (settingsValues.value['appearance']) {
      const appCfg = settingsValues.value['appearance'];
      if (typeof appCfg['body_font'] === 'string' && appCfg['body_font']) {
        localStorage.setItem('oneship-body-font', appCfg['body_font']);
      }
      if (typeof appCfg['font_scale'] === 'string' && appCfg['font_scale']) {
        localStorage.setItem('oneship-font-scale', appCfg['font_scale']);
      }
      if (typeof appCfg['quota_gauge_style'] === 'string' && appCfg['quota_gauge_style']) {
        localStorage.setItem('oneship_quota_gauge_style', appCfg['quota_gauge_style']);
        useAgentMetricsStore().setQuotaGaugeStyle(appCfg['quota_gauge_style'] as QuotaGaugeStyle);
      }
      emitUiEvent('oneship-appearance-updated');
    }

    emit('close');
  } catch (e: unknown) {
    console.error("Failed to save config:", e);
    settingsError.value = e instanceof Error ? e.message : String(e);
  } finally {
    isSavingSettings.value = false;
  }
}

function pairOf(pluginId: string, fieldKey: string): AgentPairSchema | undefined {
  const plugin = pluginSchemas.value.find(p => p.plugin_id === pluginId);
  return plugin?.agent_pairs.find(p => p.runner_key === fieldKey || p.model_key === fieldKey || p.effort_key === fieldKey);
}

/** Runner currently selected for a pair (its runner field, or the fixed runner). */
function pairRunner(pluginId: string, pair: AgentPairSchema): string {
  return pair.runner_key ? String(settingsValues.value[pluginId]?.[pair.runner_key] ?? '') : (pair.fixed_runner ?? '');
}

function getOptionsForField(field: ConfigFieldSchema, pluginId: string): SelectOption[] {
  const pair = pairOf(pluginId, field.key);
  if (field.field_type === 'runner') {
    const installed = coreStore.installedRunners;
    const current = String(settingsValues.value[pluginId]?.[field.key] ?? '');
    const list = installed && installed.length > 0
      ? runnerCatalog.runners.value.filter(r => installed.includes(r.id) || r.id === current)
      : runnerCatalog.runners.value;
    return list.map(r => ({ label: `${r.id} (${r.name})`, value: r.id }));
  }
  if (field.field_type === 'runner_model' && pair) {
    return runnerCatalog.modelsOf(pairRunner(pluginId, pair)).map(m => ({ label: m.label, value: m.id }));
  }
  if (field.field_type === 'runner_effort' && pair) {
    const runner = pairRunner(pluginId, pair);
    const modelId = String(settingsValues.value[pluginId]?.[pair.model_key] ?? '');
    const model = runnerCatalog.findModel(runner, modelId);
    const defaultLabel = model?.default_effort ? `model default (${model.default_effort})` : 'none (model has no effort)';
    return [{ label: defaultLabel, value: '' }, ...runnerCatalog.effortsOf(runner, modelId).map(e => ({ label: e, value: e }))];
  }

  return (field.options || []).map(opt => ({
    label: opt,
    value: opt,
  }));
}

/**
 * Sets a field; a runner or model change resets what no longer belongs to it. The model of
 * another runner is cleared (a model always belongs to its runner, so one must be chosen) and an
 * effort the new model does not declare falls back to the model default ('').
 */
function setFieldValue(pluginId: string, field: ConfigFieldSchema, value: ConfigPrimitive) {
  const values = settingsValues.value[pluginId];
  values[field.key] = value;
  const pair = pairOf(pluginId, field.key);
  if (!pair) return;
  const runner = pairRunner(pluginId, pair);
  if (field.field_type === 'runner' && !runnerCatalog.findModel(runner, String(values[pair.model_key] ?? ''))) {
    values[pair.model_key] = '';
  }
  if (field.field_type === 'runner' || field.field_type === 'runner_model') {
    const effort = String(values[pair.effort_key] ?? '');
    if (effort && !runnerCatalog.effortsOf(runner, String(values[pair.model_key] ?? '')).includes(effort)) {
      values[pair.effort_key] = '';
    }
  }
}

/** Pairs in the form that their runner's manifest does not declare; saving is blocked until fixed. */
const pairProblems = computed<string[]>(() => {
  const problems: string[] = [];
  for (const plugin of pluginSchemas.value) {
    for (const pair of plugin.agent_pairs) {
      const modelField = plugin.fields.find(f => f.key === pair.model_key);
      if (!modelField) continue;
      if (props.scope === 'global' && modelField.scope === 'project_only') continue;
      if (props.scope === 'project' && modelField.scope === 'global_only') continue;
      const values = settingsValues.value[plugin.plugin_id] ?? {};
      const runner = pairRunner(plugin.plugin_id, pair);
      const model = String(values[pair.model_key] ?? '');
      const effort = String(values[pair.effort_key] ?? '');
      if (!runnerCatalog.isDeclaredPair(runner, model, effort || null)) {
        problems.push(`${modelField.label}: choose a model${effort ? ' and effort' : ''} declared by runner '${runner}'`);
      }
    }
  }
  return problems;
});
</script>

<template>
  <div v-if="show" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4">
    <div class="ui-surface-base border border-[var(--border-color)] shadow-2xl max-w-4xl w-full flex flex-col h-[85vh] overflow-hidden font-mono text-xs">
      
      <!-- Header: Omnisearch-aligned title + search row -->
      <div class="ui-search-box !border-x-0 !border-t-0 px-4 py-2.5 gap-3 shrink-0">
        <div class="flex items-center gap-2 shrink-0 min-w-0 text-[var(--dim-color)]">
          <Settings :size="15" class="text-[var(--accent-color)] shrink-0" />
          <h3 class="font-bold uppercase tracking-widest text-xs truncate max-w-[14rem] sm:max-w-[20rem]">
            {{ scope === 'project' ? `Project Settings: ${project?.name}` : 'Global Settings' }}
          </h3>
          <span 
            class="text-[9px] px-2 py-0.5 border uppercase font-mono shrink-0"
            :class="scope === 'project' ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            {{ scope === 'project' ? 'rula.toml' : 'config.toml' }}
          </span>
        </div>

        <div class="h-4 w-px bg-[var(--border-color)] shrink-0" />

        <Search :size="15" class="ui-search-box__icon" />
        <AppInput
          v-model="searchQuery"
          type="text"
          placeholder="Search parameters..."
          class="!bg-transparent !border-none !h-auto !px-0 !py-0"
        />

        <div class="flex items-center gap-2 shrink-0">
          <span v-if="searchQuery" class="text-[9px] text-[var(--accent-color)] font-mono uppercase">
            {{ totalMatchCount }} found
          </span>
          <button 
            v-if="searchQuery"
            @click="searchQuery = ''"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
          >
            <X :size="13" />
          </button>
          <kbd class="ui-kbd">ESC</kbd>
          <AppTooltip text="Close" position="left">
            <button 
              @click="emit('close')"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
            >
              <X :size="16" />
            </button>
          </AppTooltip>
        </div>
      </div>

      <!-- Master-Detail Body -->
      <div class="flex flex-1 overflow-hidden">
        <!-- Left Sidebar: Category Navigation Tabs -->
        <div class="w-52 border-r border-[var(--border-color)] ui-surface-list flex flex-col p-2 gap-1 overflow-y-auto shrink-0">
          <div class="flex items-center justify-between text-[9px] uppercase font-bold text-[var(--dim-color)] px-2 py-1 tracking-wider opacity-60">
            <span>Categories</span>
            <span v-if="searchQuery" class="text-[9px] text-[var(--accent-color)] font-mono">
              {{ totalMatchCount }} found
            </span>
          </div>

          <button 
            v-for="plugin in filteredPluginSchemas" 
            :key="plugin.plugin_id"
            @click="activeSettingsTab = plugin.plugin_id"
            class="h-7 px-2.5 text-left font-bold uppercase transition-colors cursor-pointer border-l-2 text-[10px] flex items-center justify-between gap-2"
            :class="activeSettingsTab === plugin.plugin_id 
              ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10' 
              : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-white/5'"
          >
            <span class="truncate">{{ plugin.plugin_name }}</span>
            <span 
              v-if="searchQuery"
              class="text-[9px] px-1.5 py-0.2 border rounded-none font-mono shrink-0"
              :class="activeSettingsTab === plugin.plugin_id ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              {{ plugin.matchingFields.length }}
            </span>
          </button>

          <div v-if="filteredPluginSchemas.length === 0" class="p-3 text-[10px] text-[var(--dim-color)] italic text-center">
            No matching categories
          </div>
        </div>

        <!-- Right Content Area: Form Fields -->
        <div class="p-4 flex-1 overflow-y-auto flex flex-col gap-3">
          <div v-if="settingsError" class="p-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px]">
            {{ settingsError }}
          </div>

          <!-- Empty Search State -->
          <div v-if="filteredPluginSchemas.length === 0" class="flex flex-col items-center justify-center flex-1 py-12 gap-3 text-center">
            <Search :size="28" class="text-[var(--dim-color)]/40" />
            <div class="text-[10px] uppercase tracking-wider text-[var(--dim-color)]">
              No configuration parameters match <span class="text-[var(--accent-color)]">"{{ searchQuery }}"</span>
            </div>
            <button 
              @click="searchQuery = ''"
              class="text-[10px] uppercase underline text-[var(--accent-color)] hover:text-[var(--fg-color)] cursor-pointer mt-1"
            >
              Clear Search Filter
            </button>
          </div>

          <template v-for="plugin in filteredPluginSchemas" :key="plugin.plugin_id">
            <div v-if="activeSettingsTab === 'integrations' && plugin.plugin_id === 'integrations'" class="flex flex-col gap-3">
              <div class="p-3 border border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-3">
                <div class="flex flex-col gap-0.5">
                  <span class="font-bold text-[11px] text-[var(--fg-color)] uppercase tracking-wider">Host Integrations & Capabilities</span>
                  <span class="text-[10px] text-[var(--dim-color)]">
                    Only integrations detected on this machine appear in workspace tabs and settings. Configure custom binary paths below and Save to re-probe.
                  </span>
                </div>
                <button
                  type="button"
                  @click="refreshHostCapabilities"
                  :disabled="isRefreshingCapabilities"
                  class="h-7 px-2.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--fg-color)] hover:text-[var(--accent-color)] text-[9px] uppercase font-bold flex items-center gap-1.5 cursor-pointer shrink-0 disabled:opacity-50"
                >
                  <RefreshCw :size="11" :class="isRefreshingCapabilities ? 'animate-spin' : ''" />
                  <span>Refresh</span>
                </button>
              </div>

              <div v-if="coreStore.hostCapabilities" class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-wrap items-center gap-3 text-[10px]">
                <span>Runners: <strong>{{ coreStore.hostCapabilities.installed_runners.join(', ') || 'none detected' }}</strong></span>
                <span class="text-[var(--dim-color)]">|</span>
                <span>OpenDictate: <strong>{{ coreStore.hostCapabilities.opendictate_available ? 'Online' : 'Offline / Not installed' }}</strong></span>
                <span class="text-[var(--dim-color)]">|</span>
                <span>Clipboard: <strong>{{ coreStore.hostCapabilities.clipboard_available ? 'Available' : 'Missing wl-paste/xclip' }}</strong></span>
                <span class="text-[var(--dim-color)]">|</span>
                <span>Package Manager: <strong>{{ coreStore.hostCapabilities.package_manager || 'Unsupported' }}</strong></span>
              </div>

              <div
                v-for="item in (coreStore.hostCapabilities?.integrations ?? [])"
                :key="item.plugin_id"
                class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2"
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="flex items-center gap-2 min-w-0">
                    <Check v-if="item.available" :size="13" class="text-[var(--status-ok-color)] shrink-0" />
                    <AlertTriangle v-else :size="13" class="text-[var(--status-warn-color)] shrink-0" />
                    <span class="font-bold text-[11px] text-[var(--fg-color)] truncate">{{ item.label }}</span>
                  </div>
                  <span
                    class="text-[8px] px-1.5 py-0.5 border font-bold uppercase shrink-0"
                    :class="item.available
                      ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                      : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]'"
                  >
                    {{ item.available ? 'Ready' : 'Missing' }}
                  </span>
                </div>
                <p class="text-[10px] text-[var(--dim-color)]">{{ item.reason }}</p>
                <div
                  v-if="!item.available && INTEGRATION_INSTALL_GUIDES[item.plugin_id]"
                  class="px-2 py-1.5 border border-[var(--border-color)]/60 bg-[var(--bg-color)]/40 text-[9px] text-[var(--dim-color)] flex flex-col gap-0.5"
                >
                  <span>{{ INTEGRATION_INSTALL_GUIDES[item.plugin_id].summary }}</span>
                  <div class="flex flex-wrap items-center justify-between gap-2 text-[var(--fg-color)]">
                    <code>{{ INTEGRATION_INSTALL_GUIDES[item.plugin_id].installCmd }}</code>
                    <span class="text-[var(--dim-color)]">{{ INTEGRATION_INSTALL_GUIDES[item.plugin_id].sourceLabel }}</span>
                  </div>
                </div>
                <div v-for="field in editableFieldsForIntegration(item.plugin_id)" :key="field.key" class="flex flex-col gap-1 pt-1">
                  <span class="text-[9px] text-[var(--dim-color)] font-bold uppercase">{{ field.label }} ({{ field.key }})</span>
                  <AppInput
                    type="text"
                    :model-value="String(settingsValues[item.plugin_id]?.[field.key] ?? field.default_value ?? '')"
                    @update:model-value="settingsValues[item.plugin_id][field.key] = $event"
                    :placeholder="String(field.default_value ?? '')"
                  />
                </div>
              </div>
            </div>

            <div v-else-if="activeSettingsTab === plugin.plugin_id" class="flex flex-col gap-3">
              
              <div 
                v-for="field in plugin.matchingFields" 
                :key="field.key" 
                class="flex flex-col gap-2 p-3 border border-[var(--border-color)] ui-surface-card"
              >
                <!-- Label & Metadata -->
                <div class="flex justify-between items-center">
                  <div class="flex items-center gap-2">
                    <span class="font-bold text-[var(--fg-color)] text-[11px]">{{ field.label }}</span>
                    <span 
                      v-if="scope === 'project'" 
                      class="text-[9px] px-1.5 py-0.2 border uppercase font-mono"
                      :class="settingsProvenance[`${plugin.plugin_id}.${field.key}`] === 'project'
                        ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10'
                        : (settingsProvenance[`${plugin.plugin_id}.${field.key}`] === 'global' ? 'border-[var(--status-info-border)] text-[var(--status-info-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]')"
                    >
                      [{{ settingsProvenance[`${plugin.plugin_id}.${field.key}`] || 'default' }}]
                    </span>
                  </div>
                  <div class="flex items-center gap-2 text-[9px]">
                    <span class="px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">
                      {{ field.scope }}
                    </span>
                    <span class="text-[var(--dim-color)] font-mono">
                      {{ field.key }}
                    </span>
                  </div>
                </div>

                <!-- Description -->
                <p class="text-[10px] text-[var(--dim-color)] leading-relaxed">
                  {{ field.description }}
                </p>

                <!-- Dynamic Input Controls -->
                <div class="mt-1">
                  <!-- Quota Gauge Style Interactive Selector -->
                  <div v-if="field.key === 'quota_gauge_style'" class="max-w-2xl">
                    <QuotaGaugePreviewSelector
                      :model-value="String(settingsValues[plugin.plugin_id]?.[field.key] ?? field.default_value) as QuotaGaugeStyle"
                      @update:model-value="settingsValues[plugin.plugin_id][field.key] = $event"
                    />
                  </div>

                  <!-- Boolean Toggle -->
                  <div v-else-if="field.field_type === 'boolean'" class="max-w-xs">
                    <AppToggle
                      :model-value="!!settingsValues[plugin.plugin_id]?.[field.key]"
                      @update:model-value="settingsValues[plugin.plugin_id][field.key] = $event"
                      :label="settingsValues[plugin.plugin_id]?.[field.key] ? 'ENABLED' : 'DISABLED'"
                      :description="`Default: ${field.default_value ? 'Enabled' : 'Disabled'}`"
                    />
                  </div>

                  <!-- Select Enum Dropdown -->
                  <div v-else-if="field.field_type === 'select_enum' || RUNNER_FIELD_TYPES.includes(field.field_type)" class="flex flex-col gap-1 max-w-sm">
                    <div class="flex items-center gap-2">
                      <AppSelect
                        class="flex-1"
                        :model-value="String(settingsValues[plugin.plugin_id]?.[field.key] ?? field.default_value)"
                        @update:model-value="setFieldValue(plugin.plugin_id, field, $event)"
                        :options="getOptionsForField(field, plugin.plugin_id)"
                      />
                      <AppTooltip
                        v-if="plugin.plugin_id === 'voice_controller' && field.key === 'voice_name'"
                        text="Play a short voice sample with the selected voice and model"
                      >
                        <button
                          type="button"
                          @click="testSelectedVoice"
                          :disabled="isTestingVoice"
                          class="h-8 px-2.5 text-[10px] font-mono font-bold uppercase tracking-wider border border-[var(--border-color)] bg-[var(--ui-bg-base)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] disabled:opacity-50 transition-colors cursor-pointer"
                        >
                          {{ isTestingVoice ? '...' : 'Test' }}
                        </button>
                      </AppTooltip>
                    </div>
                    <span class="text-[9px] text-[var(--dim-color)]">
                      Default: {{ field.default_value }}
                    </span>
                  </div>

                  <!-- Number Input -->
                  <div v-else-if="field.field_type === 'number'" class="flex flex-col gap-1 max-w-xs">
                    <AppInput
                      type="number"
                      :model-value="Number(settingsValues[plugin.plugin_id]?.[field.key] ?? field.default_value ?? 0)"
                      @update:model-value="settingsValues[plugin.plugin_id][field.key] = Number($event)"
                      :placeholder="String(field.default_value)"
                    />
                    <span class="text-[9px] text-[var(--dim-color)]">
                      Default: {{ field.default_value }}
                    </span>
                  </div>

                  <!-- String List -->
                  <div v-else-if="field.field_type === 'string_list'" class="flex flex-col gap-1">
                    <AppInput
                      type="text"
                      :model-value="((val) => Array.isArray(val) ? val.join(', ') : '')(settingsValues[plugin.plugin_id]?.[field.key])"
                      @update:model-value="settingsValues[plugin.plugin_id][field.key] = String($event).split(',').map((s: string) => s.trim()).filter(Boolean)"
                      placeholder="Comma-separated values..."
                    />
                    <span class="text-[9px] text-[var(--dim-color)]">
                      Default: {{ JSON.stringify(field.default_value) }}
                    </span>
                  </div>

                  <!-- Standard String Input -->
                  <div v-else class="flex flex-col gap-1">
                    <AppInput
                      type="text"
                      :model-value="String(settingsValues[plugin.plugin_id]?.[field.key] ?? field.default_value ?? '')"
                      @update:model-value="settingsValues[plugin.plugin_id][field.key] = $event"
                      :placeholder="String(field.default_value ?? '')"
                    />
                    <span class="text-[9px] text-[var(--dim-color)]">
                      Default: "{{ field.default_value }}"
                    </span>
                  </div>
                </div>

              </div>

            </div>
          </template>
        </div>
      </div>

      <!-- Footer Actions -->
      <div class="ui-modal-footer gap-4">
        <span class="text-[9px] truncate max-w-[55%]">
          {{ scope === 'project' ? `${project?.path}/rula.toml` : '~/.config/one-ship/config.toml' }}
        </span>
        <div class="flex gap-2 shrink-0">
          <button 
            @click="emit('close')"
            class="h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--fg-color)] transition-colors cursor-pointer"
          >
            Cancel
          </button>
          <button 
            @click="saveSettings"
            :disabled="isSavingSettings"
            class="h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <span v-if="isSavingSettings" class="animate-pulse">Saving...</span>
            <span v-else>{{ scope === 'project' ? 'Save to rula.toml' : 'Save Global Settings' }}</span>
          </button>
        </div>
      </div>

    </div>
  </div>
</template>
