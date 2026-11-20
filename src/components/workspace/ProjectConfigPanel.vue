<script setup lang="ts">
import { ref, shallowRef, watch, computed, onMounted, onUnmounted } from 'vue';
import { Search, X, Check, AlertCircle, RefreshCw, ArrowRight, Plus, Loader2 } from '@lucide/vue';
import { AppTooltip, AppSelect, AppInput, AppToggle } from '../ui';
import type { SelectOption } from '../ui';
import { useRunnerCatalog } from '../../utils/runnerCatalog';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import { registerNavigationGuard, type NavigationTarget } from '../../utils/navigationGuard';
import { useCoreStore } from '../../stores/coreStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import type { ConfigFieldSchema, PluginConfigSchema, ProjectRecord } from '../../types';
import type { AgentPairSchema } from '../../types/generated/AgentPairSchema';
import type { ScopedConfigField } from '../../types/generated/ScopedConfigField';
import type { JsonValue } from '../../types/generated/serde_json/JsonValue';

type ConfigPrimitive = string | number | boolean | string[];

const props = defineProps<{
  project: ProjectRecord;
}>();

const emit = defineEmits<{
  (e: 'saved', name?: string): void;
  (e: 'syncSkills', project: ProjectRecord): void;
  (e: 'initGit', project: ProjectRecord): void;
}>();

const coreStore = useCoreStore();
const layoutStore = useWorkspaceLayoutStore();
const runnerCatalog = useRunnerCatalog();
const projectAgents = ref<Array<{ id: string; role: string }>>([]);

/** Field types whose options come from the runner manifests (`list_agent_runners`). */
const RUNNER_FIELD_TYPES = ['runner', 'runner_model', 'runner_effort'];

const pluginSchemas = shallowRef<PluginConfigSchema[]>([]);
const searchQuery = ref<string>('');
const activeSettingsTab = ref<string>('');
const settingsValues = ref<Record<string, Record<string, ConfigPrimitive>>>({});
const settingsProvenance = ref<Record<string, string>>({});
const isLoading = ref<boolean>(false);
const isSaving = ref<boolean>(false);
const settingsError = ref<string | null>(null);
const successMessage = ref<string | null>(null);
const loadedSnapshot = ref<string>('{}');
const externalConfigChanged = ref(false);
const showLeaveConfirm = ref(false);
const pendingNavigation = ref<NavigationTarget | null>(null);
const isResolvingLeave = ref(false);

const isDirty = computed(
  () => JSON.stringify(settingsValues.value) !== loadedSnapshot.value
);

let successMessageTimer: ReturnType<typeof setTimeout> | null = null;
let externalCheckTimer: ReturnType<typeof setTimeout> | null = null;
let unlistenProjectsUpdated: (() => void) | null = null;
let unregisterNavigationGuard: (() => void) | null = null;

// Filter plugin schemas and their fields by project scope (exclude global_only fields),
// host capability availability, and search query.
const filteredPluginSchemas = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();

  const mapped = pluginSchemas.value
    .filter((plugin) => coreStore.isIntegrationAvailable(plugin.plugin_id))
    .map((plugin) => {
      const scopedFields = plugin.fields.filter((f) => f.scope !== 'global_only');

      if (!query) {
        return {
          ...plugin,
          matchingFields: scopedFields,
          matchCount: scopedFields.length,
        };
      }

      const pluginMatches =
        plugin.plugin_name.toLowerCase().includes(query) ||
        plugin.plugin_id.toLowerCase().includes(query);
      const matchingFields = scopedFields.filter(
        (f) =>
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
    .filter((plugin) => {
      if (plugin.matchingFields.length > 0) return true;
      if (plugin.plugin_id === 'vcs') {
        return !query || plugin.plugin_name.toLowerCase().includes(query) || plugin.plugin_id.includes(query);
      }
      return false;
    });

  if (!query || 'platform skills'.includes(query) || 'skills'.includes(query)) {
    mapped.push({
      plugin_id: 'skills',
      plugin_name: 'Platform Skills',
      fields: [],
      matchingFields: [],
      matchCount: 0,
      agent_pairs: [],
    });
  }

  return mapped;
});

const totalMatchCount = computed(() => {
  return filteredPluginSchemas.value.reduce((acc, p) => acc + p.matchingFields.length, 0);
});

watch(searchQuery, () => {
  const available = filteredPluginSchemas.value;
  if (available.length > 0 && !available.some((p) => p.plugin_id === activeSettingsTab.value)) {
    activeSettingsTab.value = available[0].plugin_id;
  }
});

function formatTomlValue(val: ConfigPrimitive | JsonValue | undefined): string {
  if (typeof val === 'boolean') return val ? 'true' : 'false';
  if (typeof val === 'number') return isNaN(val) ? '0' : String(val);
  if (Array.isArray(val)) return `[${val.map((v) => formatTomlValue(v as ConfigPrimitive)).join(', ')}]`;
  return JSON.stringify(String(val ?? ''));
}

interface UpdatedConfigPayload {
  system: Record<string, ConfigPrimitive>;
  plugins: Record<string, Record<string, ConfigPrimitive>>;
}

function generateToml(configObj: UpdatedConfigPayload): string {
  const lines: string[] = [];

  if (configObj.system && Object.keys(configObj.system).length > 0) {
    lines.push('[system]');
    for (const [k, v] of Object.entries(configObj.system)) {
      if (typeof v !== 'object' || Array.isArray(v)) {
        lines.push(`${k} = ${formatTomlValue(v)}`);
      }
    }
    lines.push('');
  }

  if (configObj.plugins) {
    for (const [pluginId, pluginTable] of Object.entries(configObj.plugins)) {
      if (pluginTable && Object.keys(pluginTable).length > 0) {
        lines.push(`[plugins.${pluginId}]`);
        for (const [k, v] of Object.entries(pluginTable)) {
          if (typeof v !== 'object' || Array.isArray(v)) {
            lines.push(`${k} = ${formatTomlValue(v)}`);
          }
        }
        lines.push('');
      }
    }
  }

  return lines.join('\n').trimEnd() + '\n';
}

interface ScopedValues {
  values: Record<string, Record<string, ConfigPrimitive>>;
  provenance: Record<string, string>;
}

function buildScopedValues(scopedDetails: Record<string, ScopedConfigField>): ScopedValues {
  const values: Record<string, Record<string, ConfigPrimitive>> = {};
  const provenance: Record<string, string> = {};

  for (const plugin of pluginSchemas.value) {
    values[plugin.plugin_id] = {};
    for (const field of plugin.fields) {
      if (field.scope === 'global_only') continue;
      const fullKey = `${plugin.plugin_id}.${field.key}`;
      const detail = scopedDetails[fullKey];
      if (detail) {
        values[plugin.plugin_id][field.key] = detail.value as ConfigPrimitive;
        provenance[fullKey] = detail.provenance;
      } else {
        values[plugin.plugin_id][field.key] = field.default_value as ConfigPrimitive;
        provenance[fullKey] = 'default';
      }
    }
  }

  return { values, provenance };
}

async function loadSettings() {
  if (!props.project?.path) return;
  isLoading.value = true;
  settingsError.value = null;
  settingsProvenance.value = {};

  try {
    await runnerCatalog.loadRunnerCatalog();
    pluginSchemas.value = await daemonCall('get_plugin_schemas', {});
    try {
      const agentState = await daemonCall('get_project_agent_state', {
        projectPath: props.project.path,
        projectName: props.project.name,
      });
      projectAgents.value = agentState.agents.map((a) => ({ id: a.id, role: a.role }));
    } catch (agentErr: unknown) {
      projectAgents.value = [];
      reportAsyncFailure('ProjectConfigPanel.loadProjectAgents', agentErr);
    }

    const scopedDetails = await daemonCall('get_scoped_project_config', {
      projectPath: props.project.path,
    });
    const { values, provenance } = buildScopedValues(scopedDetails);

    settingsValues.value = values;
    settingsProvenance.value = provenance;
    loadedSnapshot.value = JSON.stringify(values);
    externalConfigChanged.value = false;

    const available = filteredPluginSchemas.value;
    if (available.length > 0 && !available.some((p) => p.plugin_id === activeSettingsTab.value)) {
      activeSettingsTab.value = available[0].plugin_id;
    }
  } catch (e: unknown) {
    console.error('Failed to load project settings:', e);
    settingsError.value = e instanceof Error ? e.message : String(e);
  } finally {
    isLoading.value = false;
  }
}

function showTransientSuccess(message: string) {
  successMessage.value = message;
  if (successMessageTimer) clearTimeout(successMessageTimer);
  successMessageTimer = setTimeout(() => {
    if (successMessage.value === message) {
      successMessage.value = null;
    }
  }, 4000);
}

async function saveSettings(): Promise<boolean> {
  if (pairProblems.value.length > 0) {
    settingsError.value = pairProblems.value.join('; ');
    return false;
  }
  isSaving.value = true;
  settingsError.value = null;
  successMessage.value = null;

  try {
    const updated: UpdatedConfigPayload = { system: {}, plugins: {} };

    for (const plugin of pluginSchemas.value) {
      const targetTable =
        plugin.plugin_id === 'system'
          ? updated.system
          : (updated.plugins[plugin.plugin_id] = updated.plugins[plugin.plugin_id] || {});

      for (const field of plugin.fields) {
        if (field.scope === 'global_only') continue;
        const val = settingsValues.value[plugin.plugin_id]?.[field.key];
        targetTable[field.key] = val !== undefined ? val : (field.default_value as ConfigPrimitive);
      }
    }

    const tomlContent = generateToml(updated);

    await daemonCall('save_project_rula_config', {
      projectPath: props.project.path,
      configToml: tomlContent,
    });

    showTransientSuccess('Configuration saved to rula.toml');

    emit('saved', props.project.name);
    await coreStore.reloadProjects();
    await loadSettings();
    return true;
  } catch (e: unknown) {
    console.error('Failed to save rula.toml config:', e);
    settingsError.value = e instanceof Error ? e.message : String(e);
    return false;
  } finally {
    isSaving.value = false;
  }
}

async function discardChanges(): Promise<boolean> {
  externalConfigChanged.value = false;
  await loadSettings();
  return !isDirty.value;
}

async function checkExternalConfigChange() {
  if (!props.project?.path || isLoading.value || isSaving.value || pluginSchemas.value.length === 0) return;

  const dirtyBefore = isDirty.value;
  try {
    const scopedDetails = await daemonCall('get_scoped_project_config', {
      projectPath: props.project.path,
    });
    const { values, provenance } = buildScopedValues(scopedDetails);
    const nextJson = JSON.stringify(values);

    if (nextJson === JSON.stringify(settingsValues.value)) return;

    if (dirtyBefore || isDirty.value) {
      externalConfigChanged.value = true;
      return;
    }

    settingsValues.value = values;
    settingsProvenance.value = provenance;
    loadedSnapshot.value = nextJson;
    externalConfigChanged.value = false;
    settingsError.value = null;
    showTransientSuccess('rula.toml reloaded from disk');
  } catch (e: unknown) {
    console.error('Failed to reload rula.toml after an external change:', e);
    settingsError.value = e instanceof Error ? e.message : String(e);
    externalConfigChanged.value = true;
  }
}

function scheduleExternalConfigCheck() {
  if (externalCheckTimer) clearTimeout(externalCheckTimer);
  externalCheckTimer = setTimeout(() => {
    externalCheckTimer = null;
    void checkExternalConfigChange();
  }, 350);
}

function handleNavigationAttempt(target: NavigationTarget): boolean {
  if (!isDirty.value) return true;
  if (showLeaveConfirm.value) {
    if (!pendingNavigation.value) pendingNavigation.value = {};
    if (target.projectId !== undefined) pendingNavigation.value.projectId = target.projectId;
    if (target.workspaceTab) pendingNavigation.value.workspaceTab = target.workspaceTab;
    return false;
  }
  pendingNavigation.value = target;
  showLeaveConfirm.value = true;
  return false;
}

function applyPendingNavigation() {
  const target = pendingNavigation.value;
  pendingNavigation.value = null;
  showLeaveConfirm.value = false;
  if (!target) return;

  if (target.projectId !== undefined) {
    coreStore.setActiveProject(target.projectId, { bypassNavigationGuard: true });
  }
  if (target.workspaceTab) {
    layoutStore.setWorkspaceTab(target.workspaceTab, { bypassNavigationGuard: true });
  }
}

async function handleLeaveSave() {
  if (isResolvingLeave.value) return;
  isResolvingLeave.value = true;
  const saved = await saveSettings();
  isResolvingLeave.value = false;
  if (!saved) {
    showLeaveConfirm.value = false;
    pendingNavigation.value = null;
    return;
  }
  applyPendingNavigation();
}

async function handleLeaveDiscard() {
  if (isResolvingLeave.value) return;
  isResolvingLeave.value = true;
  const discarded = await discardChanges();
  isResolvingLeave.value = false;
  if (!discarded) {
    showLeaveConfirm.value = false;
    pendingNavigation.value = null;
    return;
  }
  applyPendingNavigation();
}

function handleLeaveCancel() {
  showLeaveConfirm.value = false;
  pendingNavigation.value = null;
}

function pairOf(pluginId: string, fieldKey: string): AgentPairSchema | undefined {
  const plugin = pluginSchemas.value.find((p) => p.plugin_id === pluginId);
  return plugin?.agent_pairs.find(
    (p) => p.runner_key === fieldKey || p.model_key === fieldKey || p.effort_key === fieldKey
  );
}

function pairRunner(pluginId: string, pair: AgentPairSchema): string {
  return pair.runner_key ? String(settingsValues.value[pluginId]?.[pair.runner_key] ?? '') : (pair.fixed_runner ?? '');
}

function getOptionsForField(field: ConfigFieldSchema, pluginId: string): SelectOption[] {
  const pair = pairOf(pluginId, field.key);
  if (field.field_type === 'project_agent') {
    return [
      { label: 'None (unset)', value: '' },
      ...projectAgents.value.map((a) => ({ label: `${a.id} — ${a.role}`, value: a.id })),
    ];
  }
  if (field.field_type === 'runner') {
    const installed = coreStore.installedRunners;
    const current = String(settingsValues.value[pluginId]?.[field.key] ?? '');
    const list = installed && installed.length > 0
      ? runnerCatalog.runners.value.filter((r) => installed.includes(r.id) || r.id === current)
      : runnerCatalog.runners.value;
    return list.map((r) => ({ label: `${r.id} (${r.name})`, value: r.id }));
  }
  if (field.field_type === 'runner_model' && pair) {
    return runnerCatalog.modelsOf(pairRunner(pluginId, pair)).map((m) => ({ label: m.label, value: m.id }));
  }
  if (field.field_type === 'runner_effort' && pair) {
    const runner = pairRunner(pluginId, pair);
    const modelId = String(settingsValues.value[pluginId]?.[pair.model_key] ?? '');
    const model = runnerCatalog.findModel(runner, modelId);
    const defaultLabel = model?.default_effort
      ? `model default (${model.default_effort})`
      : 'none (model has no effort)';
    return [
      { label: defaultLabel, value: '' },
      ...runnerCatalog.effortsOf(runner, modelId).map((e) => ({ label: e, value: e })),
    ];
  }

  return (field.options || []).map((opt) => ({
    label: opt,
    value: opt,
  }));
}

function setFieldValue(pluginId: string, field: ConfigFieldSchema, value: ConfigPrimitive) {
  const values = settingsValues.value[pluginId];
  if (!values) return;
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

const pairProblems = computed<string[]>(() => {
  const problems: string[] = [];
  for (const plugin of pluginSchemas.value) {
    for (const pair of plugin.agent_pairs) {
      const modelField = plugin.fields.find((f) => f.key === pair.model_key);
      if (!modelField || modelField.scope === 'global_only') continue;
      const values = settingsValues.value[plugin.plugin_id] ?? {};
      const runner = pairRunner(plugin.plugin_id, pair);
      const model = String(values[pair.model_key] ?? '');
      const effort = String(values[pair.effort_key] ?? '');
      if (!runnerCatalog.isDeclaredPair(runner, model, effort || null)) {
        problems.push(
          `${modelField.label}: choose a model${effort ? ' and effort' : ''} declared by runner '${runner}'`
        );
      }
    }
  }
  return problems;
});

const togglingCategory = ref<Record<string, boolean>>({});

const missingProjectIntegrations = computed(() => {
  const caps = coreStore.hostCapabilities?.integrations ?? [];
  const enabledFlags: Record<string, boolean> = {
    rea: props.project.has_rea_enabled,
    desigma: props.project.has_desigma_enabled,
    solidcraft: props.project.has_solidcraft_enabled,
    pcbcraft: props.project.has_pcbcraft_enabled,
    electronics: props.project.has_electronics_enabled,
  };
  return caps.filter((c) => !c.available && !!enabledFlags[c.plugin_id]);
});

const TOGGLEABLE_PLUGIN_IDS = new Set([
  'rea',
  'desigma',
  'solidcraft',
  'pcbcraft',
  'profiler',
  'electronics',
  'project_management',
  'semantic',
  'supervisor',
  'skills',
]);

function hasCategoryToggle(pluginId: string): boolean {
  if (pluginId === 'vcs') return !!props.project.has_git;
  return TOGGLEABLE_PLUGIN_IDS.has(pluginId);
}

function hasQuickAction(pluginId: string): boolean {
  switch (pluginId) {
    case 'electronics':
      return props.project.has_electronics_enabled;
    case 'solidcraft':
      return props.project.has_solidcraft_enabled;
    case 'pcbcraft':
      return props.project.has_pcbcraft_enabled;
    case 'project_management':
      return props.project.has_project_management_enabled;
    case 'vcs':
    case 'supervisor':
    case 'skills':
      return true;
    default:
      return false;
  }
}

function isCategoryEnabled(pluginId: string): boolean {
  switch (pluginId) {
    case 'rea':
      return props.project.has_rea_enabled;
    case 'desigma':
      return props.project.has_desigma_enabled;
    case 'solidcraft':
      return props.project.has_solidcraft_enabled;
    case 'pcbcraft':
      return props.project.has_pcbcraft_enabled;
    case 'profiler':
      return props.project.has_profiler_enabled;
    case 'electronics':
      return props.project.has_electronics_enabled;
    case 'project_management':
      return props.project.has_project_management_enabled;
    case 'semantic':
      return settingsValues.value.semantic?.enabled !== false;
    case 'vcs':
      return props.project.has_git && !props.project.hidden_tabs?.includes('git');
    case 'supervisor':
      return !props.project.hidden_tabs?.includes('terminal');
    case 'skills':
      return !props.project.hidden_tabs?.includes('skills');
    default:
      return true;
  }
}

function getCategoryToggleTooltip(pluginId: string): string {
  const enabled = isCategoryEnabled(pluginId);
  if (pluginId === 'vcs' || pluginId === 'supervisor' || pluginId === 'skills') {
    return enabled ? 'Hide workspace tab' : 'Show workspace tab';
  }
  return enabled ? 'Disable subsystem' : 'Enable subsystem';
}

async function handleCategoryToggle(pluginId: string, enabled: boolean) {
  if (!props.project || togglingCategory.value[pluginId]) return;
  togglingCategory.value[pluginId] = true;
  try {
    // A category toggle reloads the file afterwards; persist or block pending field edits first
    // so a subsystem switch cannot silently discard them. The semantic toggle saves
    // its mutated value through saveSettings below, which persists the whole form.
    if (isDirty.value && pluginId !== 'semantic') {
      const saved = await saveSettings();
      if (!saved) return;
    }
    switch (pluginId) {
      case 'rea':
        await daemonCall('toggle_project_rea_plugin', { projectPath: props.project.path, enabled });
        break;
      case 'desigma':
        await daemonCall('toggle_project_desigma_plugin', { project_path: props.project.path, enabled });
        break;
      case 'solidcraft':
        await daemonCall('toggle_project_solidcraft_plugin', { project_path: props.project.path, enabled });
        break;
      case 'pcbcraft':
        await daemonCall('toggle_project_pcbcraft_plugin', { project_path: props.project.path, enabled });
        break;
      case 'profiler':
        await daemonCall('toggle_project_profiler_plugin', { projectPath: props.project.path, enabled });
        break;
      case 'electronics':
        await daemonCall('toggle_project_electronics_plugin', { projectPath: props.project.path, enabled });
        break;
      case 'project_management':
        await daemonCall('toggle_project_management_plugin', { projectPath: props.project.path, enabled });
        break;
      case 'semantic':
        if (!settingsValues.value[pluginId]) {
          settingsValues.value[pluginId] = {};
        }
        settingsValues.value[pluginId].enabled = enabled;
        await saveSettings();
        return;
      case 'vcs':
        await coreStore.toggleProjectTabVisibility(props.project.id, 'git', enabled);
        return;
      case 'supervisor':
        await coreStore.toggleProjectTabVisibility(props.project.id, 'terminal', enabled);
        return;
      case 'skills':
        await coreStore.toggleProjectTabVisibility(props.project.id, 'skills', enabled);
        return;
    }
    await coreStore.reloadProjects();
    await loadSettings();
  } catch (err: unknown) {
    reportAsyncFailure('ProjectConfigPanel.toggleCategory', err);
    settingsError.value = err instanceof Error ? err.message : `Failed to toggle ${pluginId}`;
  } finally {
    togglingCategory.value[pluginId] = false;
  }
}

onMounted(() => {
  loadSettings();
  unlistenProjectsUpdated = onDaemonEvent('projects-updated', (projects) => {
    if (projects.some((p) => p.id === props.project?.id)) {
      scheduleExternalConfigCheck();
    }
  });
});

watch(isDirty, (dirty) => {
  if (dirty && !unregisterNavigationGuard) {
    unregisterNavigationGuard = registerNavigationGuard(handleNavigationAttempt);
  } else if (!dirty && unregisterNavigationGuard) {
    unregisterNavigationGuard();
    unregisterNavigationGuard = null;
  }
  if (!dirty && showLeaveConfirm.value) {
    applyPendingNavigation();
  }
}, { immediate: true });

watch(
  () => props.project?.path,
  () => {
    showLeaveConfirm.value = false;
    pendingNavigation.value = null;
    loadSettings();
  }
);

onUnmounted(() => {
  if (unlistenProjectsUpdated) unlistenProjectsUpdated();
  if (externalCheckTimer) clearTimeout(externalCheckTimer);
  if (successMessageTimer) clearTimeout(successMessageTimer);
  if (unregisterNavigationGuard) {
    unregisterNavigationGuard();
    unregisterNavigationGuard = null;
  }
});
</script>

<template>
  <div class="flex flex-col h-full min-h-0 w-full overflow-hidden font-mono text-xs select-text">
    <!-- Unified toolbar: parameter search + manual save/discard -->
    <div class="ui-search-box !border-x-0 !border-t-0 h-9 px-3 gap-3 shrink-0">
      <Search :size="14" class="ui-search-box__icon" />
      <AppInput
        v-model="searchQuery"
        placeholder="Filter parameters..."
        class="!bg-transparent !border-none !h-auto !px-0 !py-0"
      />
      <button
        v-if="searchQuery"
        @click="searchQuery = ''"
        class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
      >
        <X :size="12" />
      </button>

      <div class="flex items-center gap-2 shrink-0">
        <Loader2 v-if="isLoading" :size="12" class="animate-spin text-[var(--accent-color)]" />
        <span v-if="searchQuery" class="text-[9px] text-[var(--accent-color)] font-mono uppercase">
          {{ totalMatchCount }} found
        </span>
        <button
          @click="discardChanges"
          :disabled="(!isDirty && !externalConfigChanged) || isSaving || isLoading"
          class="h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--fg-color)] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Discard
        </button>
        <button
          @click="saveSettings"
          :disabled="!isDirty || isSaving || isLoading"
          class="h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Loader2 v-if="isSaving" :size="11" class="animate-spin" />
          <span>{{ isSaving ? 'Saving...' : 'Save' }}</span>
        </button>
      </div>
    </div>

    <!-- Alert Banners: Error, External Change and Success -->
    <div v-if="settingsError" class="px-3 py-2 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px] flex items-center gap-2">
      <AlertCircle :size="13" class="shrink-0" />
      <span>{{ settingsError }}</span>
    </div>
    <div v-if="externalConfigChanged" class="px-3 py-2 border-b border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[10px] flex items-center gap-2">
      <AlertCircle :size="13" class="shrink-0" />
      <span>rula.toml changed on disk. Discard loads the disk version; Save overwrites it with the in-memory version.</span>
    </div>
    <div v-if="missingProjectIntegrations.length > 0" class="px-3 py-2 border-b border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[10px] flex items-center gap-2">
      <AlertCircle :size="13" class="shrink-0" />
      <span>
        Project enables integration(s) missing on this host:
        <strong>{{ missingProjectIntegrations.map((i) => i.label).join(', ') }}</strong>.
        Tabs stay hidden until binaries are installed or paths are configured in Global Settings → Integrations.
      </span>
    </div>
    <div v-if="successMessage" class="px-3 py-2 border-b border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[10px] flex items-center gap-2">
      <Check :size="13" class="shrink-0" />
      <span>{{ successMessage }}</span>
    </div>

    <!-- Master-Detail Body -->
    <div class="flex flex-col sm:flex-row flex-1 min-h-0 overflow-hidden">
      <!-- Left Sidebar: Category Navigation Tabs (full height) -->
      <div class="w-full sm:w-56 border-b sm:border-b-0 sm:border-r border-[var(--border-color)] ui-surface-list flex flex-col shrink-0 min-h-0">
        <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-1">
          <button
            v-for="plugin in filteredPluginSchemas"
            :key="plugin.plugin_id"
            @click="activeSettingsTab = plugin.plugin_id"
            class="h-7 px-2.5 text-left font-bold uppercase transition-colors cursor-pointer border-l-2 text-[10px] flex items-center justify-between gap-2"
            :class="[
              activeSettingsTab === plugin.plugin_id
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10'
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-white/5',
              !isCategoryEnabled(plugin.plugin_id) ? 'opacity-50' : ''
            ]"
          >
            <span class="truncate">{{ plugin.plugin_name }}</span>
            <div class="flex items-center gap-1.5 shrink-0">
              <span
                v-if="searchQuery && plugin.matchingFields.length > 0"
                class="text-[9px] px-1.5 py-0.2 border rounded-none font-mono shrink-0"
                :class="activeSettingsTab === plugin.plugin_id ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
              >
                {{ plugin.matchingFields.length }}
              </span>
              <div v-if="hasCategoryToggle(plugin.plugin_id)" @click.stop>
                <AppTooltip :text="getCategoryToggleTooltip(plugin.plugin_id)" position="right">
                  <AppToggle
                    :model-value="isCategoryEnabled(plugin.plugin_id)"
                    :disabled="!!togglingCategory[plugin.plugin_id]"
                    @update:model-value="handleCategoryToggle(plugin.plugin_id, $event)"
                  />
                </AppTooltip>
              </div>
            </div>
          </button>

          <div v-if="filteredPluginSchemas.length === 0" class="p-3 text-[10px] text-[var(--dim-color)] italic text-center">
            No matching categories
          </div>
        </div>
      </div>

      <!-- Right Content Area: Form Fields -->
      <div class="p-3 flex-1 min-h-0 overflow-y-auto flex flex-col gap-3">
        <!-- Empty Search State -->
        <div v-if="filteredPluginSchemas.length === 0" class="flex flex-col items-center justify-center flex-1 py-12 gap-3 text-center">
          <Search :size="28" class="text-[var(--dim-color)]/40" />
          <div class="text-[10px] uppercase tracking-wider text-[var(--dim-color)]">
            No parameters match <span class="text-[var(--accent-color)]">"{{ searchQuery }}"</span>
          </div>
          <button
            @click="searchQuery = ''"
            class="text-[10px] uppercase underline text-[var(--accent-color)] hover:text-[var(--fg-color)] cursor-pointer mt-1"
          >
            Clear Filter
          </button>
        </div>

        <template v-for="plugin in filteredPluginSchemas" :key="plugin.plugin_id">
          <div v-if="activeSettingsTab === plugin.plugin_id" class="flex flex-col gap-3">
            <!-- Quick actions for the active category (the sidebar already identifies it) -->
            <div v-if="hasQuickAction(plugin.plugin_id)" class="flex flex-wrap items-center justify-end gap-2">
                <button
                  v-if="plugin.plugin_id === 'electronics' && project.has_electronics_enabled"
                  @click="layoutStore.setWorkspaceTab('electronics')"
                  class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                >
                  <span>Open Lab</span>
                  <ArrowRight class="w-3 h-3" />
                </button>
                <button
                  v-else-if="plugin.plugin_id === 'solidcraft' && project.has_solidcraft_enabled"
                  @click="layoutStore.setWorkspaceTab('cad')"
                  class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                >
                  <span>Open CAD</span>
                  <ArrowRight class="w-3 h-3" />
                </button>
                <button
                  v-else-if="plugin.plugin_id === 'pcbcraft' && project.has_pcbcraft_enabled"
                  @click="layoutStore.setWorkspaceTab('pcb')"
                  class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                >
                  <span>Open PCB</span>
                  <ArrowRight class="w-3 h-3" />
                </button>
                <button
                  v-else-if="plugin.plugin_id === 'project_management' && project.has_project_management_enabled"
                  @click="layoutStore.setWorkspaceTab('project')"
                  class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                >
                  <span>Open Board</span>
                  <ArrowRight class="w-3 h-3" />
                </button>
                <template v-else-if="plugin.plugin_id === 'vcs'">
                  <button
                    v-if="project.has_git"
                    @click="layoutStore.setWorkspaceTab('git')"
                    class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                  >
                    <span>View Snapshots & Diffs</span>
                    <ArrowRight class="w-3 h-3" />
                  </button>
                  <button
                    v-else
                    @click="emit('initGit', project)"
                    class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-1.5"
                  >
                    <Plus class="w-3 h-3" />
                    <span>Initialize Git Repository</span>
                  </button>
                </template>
                <button
                  v-else-if="plugin.plugin_id === 'supervisor'"
                  @click="layoutStore.setWorkspaceTab('terminal')"
                  class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1.5"
                >
                  <span>Inspect Scripts & Ports</span>
                  <ArrowRight class="w-3 h-3" />
                </button>
                <template v-else-if="plugin.plugin_id === 'skills'">
                  <button
                    @click="layoutStore.setWorkspaceTab('skills')"
                    class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1"
                  >
                    <span>Skills</span>
                    <ArrowRight class="w-3 h-3" />
                  </button>
                  <button
                    @click="emit('syncSkills', project)"
                    class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1"
                  >
                    <RefreshCw class="w-3 h-3" />
                    <span>Sync</span>
                  </button>
                </template>
            </div>

            <div
              v-if="plugin.plugin_id === 'skills'"
              class="p-3 border border-[var(--border-color)] ui-surface-card text-[10px] text-[var(--dim-color)] leading-relaxed"
            >
              Managed platform agent skills symlinked into <span class="font-mono text-[var(--fg-color)]">.agents/skills/</span> from canonical templates.
              Status: <span class="font-bold text-[var(--fg-color)]">{{ project.skill_status?.is_outdated ? 'Outdated' : (project.skill_status?.is_installed ? 'Synchronized' : 'Uninstalled') }}</span>.
            </div>

            <div
              v-if="hasCategoryToggle(plugin.plugin_id) && !isCategoryEnabled(plugin.plugin_id)"
              class="px-3 py-2 border border-[var(--border-color)] ui-surface-card text-[10px] text-[var(--dim-color)] font-mono"
            >
              Feature disabled. Enable it from the category switch on the left to configure parameters.
            </div>

            <div
              v-if="plugin.plugin_id !== 'skills' && plugin.matchingFields.filter(f => !(hasCategoryToggle(plugin.plugin_id) && f.key === 'enabled')).length === 0"
              class="p-3 border border-[var(--border-color)] ui-surface-card text-[10px] text-[var(--dim-color)] leading-relaxed"
            >
              No additional project-level parameters. Use the switch on the left to enable or hide this feature.
            </div>

            <template
              v-for="field in plugin.matchingFields"
              :key="field.key"
            >
              <div
                v-if="!(hasCategoryToggle(plugin.plugin_id) && field.key === 'enabled')"
                class="flex flex-col gap-2 p-3 border border-[var(--border-color)] ui-surface-card transition-opacity"
                :class="hasCategoryToggle(plugin.plugin_id) && !isCategoryEnabled(plugin.plugin_id) ? 'opacity-45 pointer-events-none select-none' : ''"
              >
              <!-- Label, Provenance Badge and Scope -->
              <div class="flex justify-between items-center gap-2">
                <div class="flex items-center gap-2">
                  <span class="font-bold text-[var(--fg-color)] text-[11px]">{{ field.label }}</span>
                  <span
                    class="text-[9px] px-1.5 py-0.2 border uppercase font-mono"
                    :class="settingsProvenance[`${plugin.plugin_id}.${field.key}`] === 'project'
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10'
                      : (settingsProvenance[`${plugin.plugin_id}.${field.key}`] === 'global' ? 'border-[var(--status-info-border)] text-[var(--status-info-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]')"
                  >
                    [{{ settingsProvenance[`${plugin.plugin_id}.${field.key}`] === 'project' ? 'rula.toml' : (settingsProvenance[`${plugin.plugin_id}.${field.key}`] || 'default') }}]
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
                <!-- Boolean Toggle -->
                <div v-if="field.field_type === 'boolean'" class="max-w-xs">
                  <AppToggle
                    :model-value="!!settingsValues[plugin.plugin_id]?.[field.key]"
                    @update:model-value="settingsValues[plugin.plugin_id][field.key] = $event"
                    :label="settingsValues[plugin.plugin_id]?.[field.key] ? 'ENABLED' : 'DISABLED'"
                    :description="`Default: ${field.default_value ? 'Enabled' : 'Disabled'}`"
                  />
                </div>

                <!-- Select Enum / Project Agent / Runner Dropdown -->
                <div v-else-if="field.field_type === 'select_enum' || field.field_type === 'project_agent' || RUNNER_FIELD_TYPES.includes(field.field_type)" class="flex flex-col gap-1 max-w-sm">
                  <AppSelect
                    :model-value="String(settingsValues[plugin.plugin_id]?.[field.key] ?? field.default_value)"
                    @update:model-value="setFieldValue(plugin.plugin_id, field, $event)"
                    :options="getOptionsForField(field, plugin.plugin_id)"
                  />
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
            </template>
          </div>
        </template>
      </div>
    </div>

    <!-- Unsaved changes confirmation when leaving the settings tab or switching project -->
    <div v-if="showLeaveConfirm" class="fixed inset-0 z-50 ui-overlay-backdrop flex items-center justify-center p-4">
      <div class="ui-surface-title border border-[var(--accent-color)] p-4 max-w-md w-full font-mono flex flex-col gap-3 shadow-2xl">
        <div class="flex items-center gap-1.5 border-b border-[var(--border-color)] pb-2 text-[var(--accent-color)] font-bold text-[11px] uppercase tracking-wider">
          <AlertCircle :size="13" />
          <span>Unsaved Configuration</span>
        </div>
        <p class="text-[10px] text-[var(--fg-color)] leading-relaxed">
          You have unsaved changes in <span class="font-bold text-[var(--accent-color)]">rula.toml</span>. What would you like to do before leaving?
        </p>
        <div class="flex flex-col gap-1.5 pt-1">
          <button
            type="button"
            :disabled="isResolvingLeave"
            class="w-full h-7 bg-[var(--accent-color)] hover:opacity-90 text-[var(--bg-color)] font-bold uppercase text-[9px] text-center cursor-pointer disabled:opacity-50"
            @click="handleLeaveSave"
          >
            {{ isResolvingLeave ? 'Working...' : 'Save & Continue' }}
          </button>
          <button
            type="button"
            :disabled="isResolvingLeave"
            class="w-full h-7 bg-[var(--ui-bg-card)]/40 hover:bg-[var(--ui-bg-card)] border border-[var(--border-color)] text-[var(--fg-color)] uppercase text-[9px] text-center cursor-pointer disabled:opacity-50"
            @click="handleLeaveDiscard"
          >
            Discard & Continue
          </button>
          <button
            type="button"
            :disabled="isResolvingLeave"
            class="w-full h-7 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase text-[9px] text-center cursor-pointer disabled:opacity-50"
            @click="handleLeaveCancel"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
