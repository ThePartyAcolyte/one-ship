<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { Terminal, Play, Check, X, RotateCcw } from '@lucide/vue';
import { AppSelect } from '../ui';
import type { ProjectRecord, ScriptEntry } from '../../types';

type ScriptOptionValue = string | boolean;

const props = defineProps<{
  show: boolean;
  project: ProjectRecord | null;
  script: ScriptEntry | null;
  initialOptions?: Record<string, ScriptOptionValue>;
  isRunning?: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'launch', options: Record<string, ScriptOptionValue>): void;
  (e: 'save', options: Record<string, ScriptOptionValue>): void;
}>();

const optionsState = ref<Record<string, ScriptOptionValue>>({});

function getStorageKey(): string {
  if (!props.project || !props.script) return '';
  return `oneship:script_options:${props.project.id}:${props.script.id}`;
}

function loadInitialOptions() {
  if (!props.script) return;
  const defaults: Record<string, ScriptOptionValue> = {};

  if (props.script.options) {
    for (const opt of props.script.options) {
      const defVal = typeof opt.default === 'boolean' || typeof opt.default === 'string'
        ? opt.default
        : (opt.type === 'select' ? (opt.choices?.[0] || '') : false);
      defaults[opt.key] = defVal;
    }
  }

  // Check localStorage first
  const key = getStorageKey();
  if (key) {
    try {
      const stored = localStorage.getItem(key);
      if (stored) {
        const parsed = JSON.parse(stored);
        optionsState.value = { ...defaults, ...parsed };
        return;
      }
    } catch {
      // ignore parse errors
    }
  }

  if (props.initialOptions) {
    optionsState.value = { ...defaults, ...props.initialOptions };
  } else {
    optionsState.value = { ...defaults };
  }
}

watch(
  () => [props.show, props.script?.id],
  ([show]) => {
    if (show) {
      loadInitialOptions();
    }
  },
  { immediate: true }
);

function persistOptions() {
  const key = getStorageKey();
  if (key) {
    try {
      localStorage.setItem(key, JSON.stringify(optionsState.value));
    } catch (e) {
      console.error('Failed to save script options to localStorage', e);
    }
  }
}

function handleToggleBoolean(key: string) {
  optionsState.value[key] = !optionsState.value[key];
  persistOptions();
}

function handleOptionChange() {
  persistOptions();
}

function resetDefaults() {
  if (!props.script?.options) return;
  const defaults: Record<string, ScriptOptionValue> = {};
  for (const opt of props.script.options) {
    defaults[opt.key] = typeof opt.default === 'boolean' || typeof opt.default === 'string'
      ? opt.default
      : (opt.type === 'select' ? (opt.choices?.[0] || '') : false);
  }
  optionsState.value = defaults;
  persistOptions();
}

function handleSave() {
  persistOptions();
  emit('save', { ...optionsState.value });
  emit('close');
}

function handleLaunch() {
  persistOptions();
  emit('launch', { ...optionsState.value });
  emit('close');
}

const computedCommandPreview = computed(() => {
  if (!props.script) return '';
  const parts = [props.script.command, ...(props.script.args || [])];

  if (props.script.options) {
    for (const opt of props.script.options) {
      const val = optionsState.value[opt.key];
      if (opt.type === 'boolean' && val && opt.flag) {
        parts.push(opt.flag);
      } else if (opt.type === 'select' && val && opt.flag) {
        parts.push(opt.flag, String(val));
      }
    }
  }

  return parts.join(' ');
});
</script>

<template>
  <div v-if="show && script" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono">
    <div class="ui-surface-modal border border-[var(--border-color)] p-5 max-w-md w-full flex flex-col gap-4 shadow-2xl text-xs max-h-[90vh] overflow-y-auto">
      <!-- Modal Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-2">
        <h3 class="font-bold uppercase tracking-widest text-[var(--accent-color)] flex items-center gap-2">
          <Terminal :size="15" />
          <span>{{ isRunning ? 'Configure' : 'Launch' }}: {{ script.name }}</span>
        </h3>
        <button 
          @click="emit('close')" 
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          <X :size="15" />
        </button>
      </div>

      <!-- Command Line Preview -->
      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between text-[10px] text-[var(--dim-color)] uppercase tracking-wider">
          <span>Command</span>
          <span v-if="script.is_persistent" class="text-[9px] text-[var(--accent-color)]">PERSISTENT SERVICE</span>
          <span v-else class="text-[9px] text-[var(--dim-color)]">TASK</span>
        </div>
        <div class="p-2 border border-[var(--border-color)]/60 bg-black/30 text-[10px] text-[var(--fg-color)] font-mono break-all select-text">
          <span class="text-[var(--dim-color)] mr-1">$</span>{{ computedCommandPreview }}
        </div>
      </div>

      <!-- Configurable Options -->
      <div v-if="script.options?.length" class="flex flex-col gap-2.5 pt-1">
        <div class="flex items-center justify-between text-[10px] text-[var(--dim-color)] uppercase tracking-wider border-b border-[var(--border-color)]/30 pb-1">
          <span>Configurable Options</span>
          <button 
            @click="resetDefaults" 
            class="text-[9px] text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1"
          >
            <RotateCcw :size="9" />
            <span>Reset Defaults</span>
          </button>
        </div>

        <div class="flex flex-col gap-2">
          <template v-for="opt in script.options" :key="opt.key">
            <!-- Boolean Option: Compact Dot/Square Toggle -->
            <div 
              v-if="opt.type === 'boolean'"
              class="flex items-center justify-between p-2 border border-[var(--border-color)] bg-black/20 hover:border-[var(--accent-color)] cursor-pointer select-none transition-colors"
              @click="handleToggleBoolean(opt.key)"
            >
              <div class="flex flex-col gap-0.5">
                <span class="text-xs font-medium text-[var(--fg-color)]">{{ opt.label || opt.key }}</span>
                <span v-if="opt.flag" class="text-[9px] text-[var(--dim-color)] font-mono">{{ opt.flag }}</span>
              </div>
              <div 
                class="w-3.5 h-3.5 border flex items-center justify-center transition-all shrink-0 ml-2"
                :class="optionsState[opt.key] 
                  ? 'border-[var(--accent-color)] bg-[var(--accent-color)]' 
                  : 'border-[var(--dim-color)] bg-transparent hover:border-[var(--fg-color)]'"
              >
                <!-- Filled accent dot when active -->
                <span v-if="optionsState[opt.key]" class="w-1.5 h-1.5 bg-black"></span>
              </div>
            </div>

            <!-- Select Option -->
            <div 
              v-else-if="opt.type === 'select'"
              class="flex flex-col gap-1 p-2 border border-[var(--border-color)] bg-black/20"
            >
              <div class="flex items-center justify-between">
                <span class="text-xs font-medium text-[var(--fg-color)]">{{ opt.label || opt.key }}</span>
                <span v-if="opt.flag" class="text-[9px] text-[var(--dim-color)] font-mono">{{ opt.flag }}</span>
              </div>
              <AppSelect
                :model-value="String(optionsState[opt.key] ?? '')"
                :options="(opt.choices || []).map((c: string) => ({ label: c, value: c }))"
                @update:model-value="(val: string) => { optionsState[opt.key] = val; handleOptionChange(); }"
              />
            </div>
          </template>
        </div>
      </div>

      <div v-else class="p-3 border border-dashed border-[var(--border-color)] text-center text-xs text-[var(--dim-color)] opacity-50 italic">
        No configurable options defined for this script.
      </div>

      <!-- Action Footer -->
      <div class="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border-color)]">
        <button 
          @click="emit('close')" 
          class="px-3 py-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--fg-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer text-xs"
        >
          Cancel
        </button>

        <button 
          v-if="isRunning"
          @click="handleSave"
          class="px-4 py-1.5 bg-[var(--accent-color)] text-black font-bold hover:brightness-110 transition-all cursor-pointer text-xs flex items-center gap-1.5"
        >
          <Check :size="13" />
          <span>Save Options</span>
        </button>

        <button 
          v-else
          @click="handleLaunch" 
          class="px-4 py-1.5 bg-[var(--accent-color)] text-black font-bold hover:brightness-110 transition-all cursor-pointer text-xs flex items-center gap-1.5"
        >
          <Play :size="13" />
          <span>{{ script.is_persistent ? 'Start Service' : 'Run Task' }}</span>
        </button>
      </div>
    </div>
  </div>
</template>
