<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { ChevronDown, Cpu, User, X } from '@lucide/vue';
import type { ModelEffortOption } from './AppModelEffortSelect.vue';
import type { SelectOption } from './AppSelect.vue';
import AppTooltip from './AppTooltip.vue';

const props = withDefaults(
  defineProps<{
    runner: string;
    model: string;
    effort?: string;
    accountId?: string;
    runnerOptions: SelectOption[];
    models: ModelEffortOption[];
    accountOptions: Array<{ label: string; value: string; isRecommended?: boolean }>;
    disabled?: boolean;
    pendingConfigUpdate?: boolean;
    mountTarget?: string;
  }>(),
  {
    effort: '',
    accountId: '',
    disabled: false,
    pendingConfigUpdate: false,
    mountTarget: '#chat-messages-modal-mount',
  }
);

const emit = defineEmits<{
  (e: 'update:runner', value: string): void;
  (e: 'update:model', value: string): void;
  (e: 'update:effort', value: string): void;
  (e: 'update:accountId', value: string): void;
  (e: 'open'): void;
  (e: 'cancel'): void;
  (e: 'change', payload: { runner: string; model: string; effort: string; accountId: string }): void;
}>();

const isOpen = ref(false);
const activeStep = ref<1 | 2 | 3>(1);

function toggleModal() {
  if (props.disabled) return;
  if (isOpen.value) {
    closeModal();
    return;
  }
  isOpen.value = true;
  activeStep.value = 1;
  emit('open');
}

function closeModal() {
  if (!isOpen.value) return;
  isOpen.value = false;
  emit('cancel');
}

const isTargetInChat = computed(() => {
  if (typeof document !== 'undefined' && props.mountTarget) {
    return !!document.querySelector(props.mountTarget);
  }
  return false;
});

const resolvedMountTarget = computed(() => {
  return isTargetInChat.value ? props.mountTarget : 'body';
});

// ── DISPLAY LABELS ────────────────────────────────────────────────
const currentRunnerLabel = computed(() => {
  const r = props.runnerOptions.find(opt => opt.value === props.runner);
  return r?.label || props.runner.toUpperCase() || 'RUNNER';
});

const currentModelObj = computed(() => {
  return props.models.find(m => m.id === props.model);
});

const currentModelLabel = computed(() => {
  return currentModelObj.value?.label || props.model || 'Model';
});

const currentAccountLabel = computed(() => {
  const acc = props.accountOptions.find(opt => opt.value === props.accountId);
  return acc?.label || (props.accountId ? 'Account' : 'Default Key');
});

// ── STEP ACTIONS (Instant Click & Advance) ──────────────────────────
function selectRunner(rVal: string) {
  emit('update:runner', rVal);
  activeStep.value = 2;
}

function selectModel(m: ModelEffortOption, customEffort?: string) {
  const effortToSet = customEffort !== undefined
    ? customEffort
    : (m.default_effort || m.effort_levels?.[0] || '');
  emit('update:model', m.id);
  emit('update:effort', effortToSet);
  activeStep.value = 3;
}

function selectAccount(accVal: string) {
  emit('update:accountId', accVal);
  emit('change', {
    runner: props.runner,
    model: props.model,
    effort: props.effort || '',
    accountId: accVal,
  });
  isOpen.value = false;
}

function handleKeyDown(event: KeyboardEvent) {
  if (event.key === 'Escape' && isOpen.value) {
    closeModal();
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeyDown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown);
});
</script>

<template>
  <div class="relative inline-flex font-mono text-[9px] select-none min-w-0 max-w-full">
    <!-- Compact Consolidated Trigger Button in Toolbox -->
    <AppTooltip text="Configure runner, model and account">
      <button
        type="button"
        @click="toggleModal"
        :disabled="disabled"
        class="h-6 px-2 bg-[var(--bg-color)] border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[8px] font-mono text-[var(--fg-color)] flex items-center justify-between gap-1.5 transition-colors cursor-pointer min-w-0 max-w-full"
        :class="[
          isOpen ? 'border-[var(--accent-color)] ring-1 ring-[var(--accent-color)]/30' : '',
          disabled ? 'opacity-40 cursor-not-allowed' : ''
        ]"
      >
        <div class="flex items-center gap-1.5 truncate min-w-0">
          <!-- Runner Badge -->
          <span class="px-1 py-0.2 bg-[var(--ui-bg-card)] border border-[var(--border-color)] text-[8px] font-bold text-[var(--accent-color)] uppercase shrink-0">
            {{ currentRunnerLabel }}
          </span>

          <!-- Model Name -->
          <span class="truncate font-bold text-[var(--fg-color)]">
            {{ currentModelLabel }}
          </span>

          <!-- Effort level tag if active -->
          <span v-if="effort" class="text-[8px] text-[var(--dim-color)] shrink-0">
            ({{ effort }})
          </span>

          <span class="text-[var(--dim-color)] opacity-40 shrink-0">•</span>

          <!-- Account Label -->
          <span class="truncate text-[var(--dim-color)] text-[8px]">
            {{ currentAccountLabel }}
          </span>
        </div>

        <!-- Chevron Dropdown Arrow -->
        <ChevronDown 
          :size="10" 
          class="text-[var(--dim-color)] shrink-0 transition-transform duration-150 ml-1"
          :class="{ 'rotate-180 text-[var(--accent-color)]': isOpen }" 
        />
      </button>
    </AppTooltip>

    <!-- Modal Teleported to the Chat Messages Viewport Mount -->
    <Teleport v-if="isOpen" :to="resolvedMountTarget">
      <div
        class="z-50 ui-overlay-backdrop flex items-center justify-center p-3 font-mono text-[9px] select-none"
        :class="isTargetInChat ? 'absolute inset-0' : 'fixed inset-0'"
        @click.self="closeModal"
      >
        <div class="w-[94%] max-w-[390px] max-h-[88%] bg-[var(--bg-color)] border border-[var(--border-color)] shadow-2xl flex flex-col overflow-hidden">
          <!-- ── MODAL HEADER ───────────────────────────────────── -->
          <div class="px-3 py-2 bg-[var(--ui-bg-card)] border-b border-[var(--border-color)] flex items-center justify-between shrink-0">
            <div class="flex items-center gap-2">
              <span class="font-bold text-[9px] uppercase tracking-wider text-[var(--fg-color)]">
                Execution Config
              </span>
              <span class="text-[8px] px-1 py-0.2 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">
                Step {{ activeStep }}/3
              </span>
            </div>

            <AppTooltip text="Close (Esc)">
              <button
                type="button"
                @click="closeModal"
                class="p-0.5 border border-transparent hover:border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
              >
                <X :size="12" />
              </button>
            </AppTooltip>
          </div>

          <!-- ── BREADCRUMB STEP TABS ────────────────────────────── -->
          <div class="px-3 py-1.5 bg-black/30 border-b border-[var(--border-color)]/70 flex items-center justify-between text-[8px] uppercase tracking-wider shrink-0">
            <div class="flex items-center gap-1">
              <button
                type="button"
                @click="activeStep = 1"
                class="px-1.5 py-0.5 border cursor-pointer transition-colors"
                :class="activeStep === 1 
                  ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                  : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                1. Runner
              </button>
              <span class="text-[var(--dim-color)] opacity-40">></span>
              <button
                type="button"
                @click="activeStep = 2"
                class="px-1.5 py-0.5 border cursor-pointer transition-colors"
                :class="activeStep === 2 
                  ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                  : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                2. Model
              </button>
              <span class="text-[var(--dim-color)] opacity-40">></span>
              <button
                type="button"
                @click="activeStep = 3"
                class="px-1.5 py-0.5 border cursor-pointer transition-colors"
                :class="activeStep === 3 
                  ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                  : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                3. Account
              </button>
            </div>
          </div>

          <!-- ── STEP 1: RUNNER SELECTION ─────────────────────────── -->
          <div v-if="activeStep === 1" class="flex-1 overflow-y-auto p-3 flex flex-col gap-1.5">
            <div class="text-[8px] text-[var(--dim-color)] uppercase tracking-wider mb-0.5 font-bold">
              Select Runner Engine:
            </div>
            <div
              v-for="r in runnerOptions"
              :key="r.value"
              @click="selectRunner(r.value)"
              class="p-2 border transition-all cursor-pointer flex items-center justify-between group"
              :class="r.value === runner 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)] font-bold' 
                : 'border-[var(--border-color)]/80 bg-black/20 hover:border-[var(--accent-color)]/60 text-[var(--fg-color)]'"
            >
              <div class="flex items-center gap-2">
                <Cpu :size="13" :class="r.value === runner ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)]'" />
                <span class="text-[9px]">{{ r.label }}</span>
              </div>
              <span v-if="r.value === runner" class="text-[8px] text-[var(--accent-color)] font-bold">[ACTIVE]</span>
              <span v-else class="text-[8px] text-[var(--dim-color)] opacity-50 group-hover:opacity-100">></span>
            </div>
          </div>

          <!-- ── STEP 2: MODEL & REASONING EFFORT SELECTION ────────── -->
          <div v-else-if="activeStep === 2" class="flex-1 overflow-y-auto p-3 flex flex-col gap-1.5">
            <div class="text-[8px] text-[var(--dim-color)] uppercase tracking-wider mb-0.5 font-bold flex items-center justify-between">
              <span>Select Model ({{ currentRunnerLabel }}):</span>
              <span class="text-[8px] text-[var(--accent-color)]">Click to pick & proceed</span>
            </div>

            <div
              v-for="m in models"
              :key="m.id"
              class="p-2 border transition-all flex flex-col gap-1 group"
              :class="m.id === model 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)]' 
                : 'border-[var(--border-color)]/80 bg-black/20 hover:border-[var(--accent-color)]/60 text-[var(--fg-color)]'"
            >
              <!-- Model Row: Click selects model with default effort and advances to step 3 -->
              <div 
                @click="selectModel(m)"
                class="flex items-center justify-between cursor-pointer w-full"
              >
                <div class="flex items-center gap-2 truncate flex-1 min-w-0">
                  <span class="text-[8px]" :class="m.id === model ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)] opacity-50'">
                    {{ m.id === model ? '●' : '○' }}
                  </span>
                  <span class="truncate font-bold text-[9px]">{{ m.label }}</span>
                </div>
                <span v-if="m.id === model && !m.effort_levels?.length" class="text-[8px] text-[var(--accent-color)] font-bold ml-1 shrink-0">
                  [✓]
                </span>
                <span v-else-if="m.id !== model" class="text-[8px] text-[var(--dim-color)] opacity-50 group-hover:opacity-100">></span>
              </div>

              <!-- Effort Levels Selector (Pills clickable directly) -->
              <div 
                v-if="m.effort_levels && m.effort_levels.length > 0"
                class="flex items-center gap-1 pl-4 pt-1 border-t border-[var(--border-color)]/30 mt-0.5"
              >
                <span class="text-[8px] text-[var(--dim-color)] uppercase mr-1">Effort:</span>
                <button
                  v-for="lvl in m.effort_levels"
                  :key="lvl"
                  type="button"
                  @click.stop="selectModel(m, lvl)"
                  class="px-1.5 py-0.5 border text-[8px] font-bold uppercase transition-colors cursor-pointer"
                  :class="(m.id === model && (effort || m.default_effort) === lvl)
                    ? 'border-[var(--accent-color)] bg-[var(--accent-color)] text-black font-extrabold'
                    : 'border-[var(--border-color)] bg-[var(--bg-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--fg-color)]'"
                >
                  {{ lvl }}
                </button>
              </div>
            </div>

            <div v-if="models.length === 0" class="p-3 text-center text-[var(--dim-color)] italic">
              No models available for {{ currentRunnerLabel }}
            </div>
          </div>

          <!-- ── STEP 3: ACCOUNT SELECTION ────────────────────────── -->
          <div v-else-if="activeStep === 3" class="flex-1 overflow-y-auto p-3 flex flex-col gap-1.5">
            <div class="text-[8px] text-[var(--dim-color)] uppercase tracking-wider mb-0.5 font-bold flex items-center justify-between">
              <span>Select Account:</span>
              <span class="text-[8px] text-[var(--accent-color)]">Click to finish</span>
            </div>

            <div
              v-for="acc in accountOptions"
              :key="acc.value"
              @click="selectAccount(acc.value)"
              class="p-2 border transition-all cursor-pointer flex items-center justify-between group"
              :class="acc.value === accountId 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)] font-bold' 
                : 'border-[var(--border-color)]/80 bg-black/20 hover:border-[var(--accent-color)]/60 text-[var(--fg-color)]'"
            >
              <div class="flex items-center gap-2 truncate min-w-0 flex-1">
                <User :size="12" :class="acc.value === accountId ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)]'" />
                <span class="truncate text-[9px]">{{ acc.label }}</span>
              </div>

              <div class="flex items-center gap-1 shrink-0 ml-1.5">
                <span v-if="acc.isRecommended" class="text-[8px] text-[var(--status-ok-color)] font-bold uppercase tracking-wider px-1 py-0.2 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)]">
                  [REC]
                </span>
                <span v-if="acc.value === accountId" class="text-[8px] text-[var(--accent-color)] font-bold">
                  [✓]
                </span>
              </div>
            </div>

            <div v-if="accountOptions.length === 0" class="p-3 text-center text-[var(--dim-color)] italic">
              No accounts configured
            </div>
          </div>

          <!-- ── MODAL FOOTER ────────────────────────────────────── -->
          <div class="px-3 py-1.5 bg-[var(--ui-bg-card)]/60 border-t border-[var(--border-color)] flex items-center justify-between text-[8px] text-[var(--dim-color)] shrink-0">
            <span>Click option to select & advance</span>
            <span>Esc or click backdrop to cancel</span>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
