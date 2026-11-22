<script setup lang="ts">
import { ref, watch, computed, type Component } from 'vue';
import { BrainCircuit, Zap, Terminal, X, Play, Loader2, RefreshCw } from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { useCoreStore } from '../../stores/coreStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { AppSelect, AppTooltip } from '../ui';
import { useAgentPairPicker } from '../../utils/runnerCatalog';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  show: boolean;
  project: ProjectRecord | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'initialized'): void;
}>();

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();

const isMigration = computed(() => {
  return !!props.project?.has_config || !!props.project?.skill_status?.is_installed || !!props.project?.skill_status?.is_outdated;
});

type InitMode = 'unattended' | 'supervised';
const selectedMode = ref<InitMode>('supervised');
const isSubmitting = ref(false);
const errorMessage = ref<string | null>(null);

// Runner, model and effort come only from the runner manifests; the initializer pair of the
// configuration is preselected.
const pairPicker = useAgentPairPicker();
const {
  runner: selectedRunner,
  model: selectedModel,
  effort: selectedEffort,
  runnerOptions,
  modelOptions,
  effortOptions,
} = pairPicker;

const missingRunnerGate = computed(() => coreStore.hostCapabilities !== null && !coreStore.hasAnyRunner);

function openSetupAndClose() {
  coreStore.requestOpenSetupModal();
  emit('close');
}

watch(
  () => [props.show, props.project],
  async ([show]) => {
    if (show && props.project) {
      errorMessage.value = null;
      isSubmitting.value = false;
      try {
        await pairPicker.loadConfiguredPair('init');
      } catch (e: unknown) {
        errorMessage.value = `Cannot load the initializer runner/model: ${String(e)}`;
      }
    }
  },
  { immediate: true }
);

const modes = computed<{ id: InitMode; title: string; desc: string; icon: Component; tag: string }[]>(() => [
  {
    id: 'supervised',
    title: 'Supervised (Chat Guided)',
    desc: isMigration.value
      ? 'Applies Tier 1 deterministic migration, then launches interactive guided migration inside the project workspace via Chat Overlay.'
      : 'Launches the Project Initializer inside the project workspace with interactive Chat Overlay dialogue for stack review, persona setup, and flag emission.',
    icon: Terminal,
    tag: 'RECOMMENDED'
  },
  {
    id: 'unattended',
    title: 'Unsupervised (Autonomous)',
    desc: isMigration.value
      ? 'Applies Tier 1 deterministic migration, then executes autonomous stack & script alignment in the background.'
      : 'Fully autonomous bootstrap inside the project workspace. Analyzes stack, configures rula.toml, records Genesis Milestone, and emits initialization flags.',
    icon: Zap,
    tag: 'AUTONOMOUS'
  }
]);

async function handleStart() {
  if (!props.project) return;
  if (!pairPicker.isValid.value) {
    errorMessage.value = 'Choose a runner and one of its models';
    return;
  }
  isSubmitting.value = true;
  errorMessage.value = null;

  try {
    if (isMigration.value) {
      coreStore.addStatusLog(`Running deterministic migration for ${props.project.name}...`);
      await daemonCall('migrate_project_deterministic', { projectPath: props.project.path });
    }

    const actionLabel = isMigration.value ? 'project migration' : 'project initialization';
    coreStore.addStatusLog(`Starting ${actionLabel} (${selectedMode.value}) for ${props.project.name}...`);
    coreStore.setProjectInitializing(props.project.id, true, `Starting ${selectedMode.value} ${actionLabel}...`);

    const res = await daemonCall('initialize_project_agent', {
      projectId: props.project.id,
      mode: selectedMode.value,
      isRetry: false,
      runner: selectedRunner.value,
      model: selectedModel.value,
      effort: selectedEffort.value || null,
    });

    coreStore.addStatusLog(res);

    if (selectedMode.value === 'supervised') {
      await dialogueStore.openOverlay('initializer', props.project.path);
    }

    emit('initialized');
    emit('close');
  } catch (err: unknown) {
    console.error('Project lifecycle operation error:', err);
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isSubmitting.value = false;
  }
}
</script>

<template>
  <div 
    v-if="show && project" 
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
    @click.self="emit('close')"
  >
    <div class="ui-surface-modal border border-[var(--border-color)] w-full max-w-xl flex flex-col shadow-2xl overflow-hidden">
      <!-- Header Bar -->
      <div class="p-3 border-b border-[var(--border-color)] flex items-center justify-between bg-black/40 gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <BrainCircuit :size="16" class="text-[var(--accent-color)] shrink-0" />
          <span class="text-xs font-bold text-[var(--accent-color)] uppercase tracking-wider shrink-0">
            {{ isMigration ? 'PROJECT MIGRATION & UPGRADE' : 'INITIALIZE ORCHESTRATION' }}
          </span>
          <span class="text-xs text-[var(--dim-color)] truncate">
            // {{ project.name }}
          </span>
        </div>
        <AppTooltip text="Close" position="left">
          <button 
            @click="emit('close')"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors p-1 cursor-pointer shrink-0"
          >
            <X :size="14" />
          </button>
        </AppTooltip>
      </div>

      <!-- Content Area -->
      <div class="p-4 flex flex-col gap-4 overflow-y-auto max-h-[75vh]">
        <!-- Instructions / Description -->
        <div class="text-[11px] text-[var(--dim-color)] leading-relaxed border-l-2 border-[var(--accent-color)] pl-3">
          Bootstrap agent personas, services, and tasks for this workspace using an automated orchestration builder. Select your preferred execution strategy:
        </div>

        <!-- Mode Cards -->
        <div class="flex flex-col gap-2">
          <div 
            v-for="mode in modes" 
            :key="mode.id"
            @click="selectedMode = mode.id"
            class="border p-3 flex items-start gap-3 cursor-pointer transition-all"
            :class="[
              selectedMode === mode.id 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)]' 
                : 'border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] bg-black/20'
            ]"
          >
            <!-- Radio Indicator -->
            <div class="mt-0.5 shrink-0 flex items-center justify-center">
              <div 
                class="w-3.5 h-3.5 border flex items-center justify-center transition-colors"
                :class="selectedMode === mode.id ? 'border-[var(--accent-color)] bg-[var(--accent-color)]' : 'border-[var(--border-color)]'"
              >
                <div v-if="selectedMode === mode.id" class="w-1.5 h-1.5 bg-[var(--bg-color)]" />
              </div>
            </div>

            <!-- Mode Details -->
            <div class="flex-1 min-w-0 flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <component :is="mode.icon" :size="13" :class="selectedMode === mode.id ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)]'" />
                  <span class="text-xs font-bold tracking-wide" :class="selectedMode === mode.id ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]'">
                    {{ mode.title }}
                  </span>
                </div>
                <span 
                  class="text-[9px] px-1.5 py-0.2 border uppercase tracking-wider font-mono"
                  :class="selectedMode === mode.id ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)] opacity-60'"
                >
                  {{ mode.tag }}
                </span>
              </div>
              <p class="text-[10px] leading-relaxed opacity-80">
                {{ mode.desc }}
              </p>
            </div>
          </div>
        </div>

        <!-- Execution Runner & Model -->
        <div class="border border-[var(--border-color)] p-3 bg-black/20 flex flex-col gap-3">
          <div class="flex items-center justify-between border-b border-[var(--border-color)]/60 pb-2">
            <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)]">
              EXECUTION RUNNER, MODEL &amp; REASONING EFFORT
            </span>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <!-- Runner Selection -->
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
                Runner CLI
              </label>
              <AppSelect
                v-model="selectedRunner"
                :options="runnerOptions"
              />
            </div>

            <!-- Model Selection -->
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
                Model
              </label>
              <AppSelect
                v-model="selectedModel"
                :options="modelOptions"
              />
            </div>

            <!-- Reasoning Effort Selection -->
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
                Reasoning Effort
              </label>
              <AppSelect
                v-if="effortOptions.length > 0"
                v-model="selectedEffort"
                :options="effortOptions"
              />
              <AppTooltip v-else text="This model uses fixed reasoning effort (not configurable)." position="top">
                <div class="h-8 border border-[var(--border-color)] bg-black/20 flex items-center px-2 text-[10px] text-[var(--dim-color)] opacity-60">
                  Fixed / Default
                </div>
              </AppTooltip>
            </div>
          </div>
        </div>

        <!-- Missing Runner Gate Banner -->
        <div
          v-if="missingRunnerGate"
          class="p-2.5 bg-[var(--status-warn-bg)] border border-[var(--status-warn-border)] text-[var(--status-warn-color)] text-[10px] flex items-center justify-between gap-2"
        >
          <span>Install at least one AI CLI runner before initializing orchestration.</span>
          <button
            type="button"
            @click="openSetupAndClose"
            class="px-2 py-1 border border-[var(--status-warn-border)] bg-[var(--bg-color)] text-[var(--status-warn-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] text-[9px] font-bold uppercase shrink-0 cursor-pointer"
          >
            Setup &amp; Readiness
          </button>
        </div>

        <!-- Error Message -->
        <div v-if="errorMessage" class="p-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px] leading-tight">
          {{ errorMessage }}
        </div>
      </div>

      <!-- Action Footer -->
      <div class="p-3 border-t border-[var(--border-color)] flex items-center justify-between bg-black/40">
        <button
          type="button"
          @click="emit('close')"
          :disabled="isSubmitting"
          class="border border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] px-3 py-1.5 text-xs transition-colors cursor-pointer"
        >
          CANCEL
        </button>

        <button
          type="button"
          @click="handleStart"
          :disabled="isSubmitting || missingRunnerGate"
          class="border border-[var(--accent-color)] bg-[var(--accent-color)]/20 hover:bg-[var(--accent-color)]/30 text-[var(--accent-color)] font-bold px-4 py-1.5 text-xs transition-colors cursor-pointer flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Loader2 v-if="isSubmitting" :size="13" class="animate-spin" />
          <RefreshCw v-else-if="isMigration" :size="13" />
          <Play v-else :size="13" />
          <span>{{ isMigration ? 'START MIGRATION' : 'START INITIALIZATION' }}</span>
        </button>
      </div>
    </div>
  </div>
</template>
