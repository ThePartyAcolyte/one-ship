<script setup lang="ts">
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Database, Rocket, X, Flame } from '@lucide/vue';
import { AppInput } from '../ui';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'confirm', payload: { project: ProjectRecord; purgeTraces: boolean }): void;
}>();

const { t } = useI18n();

type PurgeStrategy = 'database_only' | 'space_him';
const selectedStrategy = ref<PurgeStrategy>('database_only');
const deleteConfirmText = ref('');

watch(() => props.project, () => {
  deleteConfirmText.value = '';
  selectedStrategy.value = 'database_only';
});

function handleConfirm() {
  if (props.project && deleteConfirmText.value === props.project.name) {
    emit('confirm', {
      project: props.project,
      purgeTraces: selectedStrategy.value === 'space_him',
    });
  }
}
</script>

<template>
  <div 
    v-if="project" 
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
    @click.self="emit('close')"
  >
    <div class="bg-[var(--bg-color)] border border-[var(--border-color)] p-6 max-w-lg w-full flex flex-col gap-4 shadow-2xl">
      <!-- Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-3">
        <div class="flex items-center gap-2">
          <Flame class="text-[var(--status-error-color)] shrink-0" :size="16" />
          <h3 class="text-xs font-bold uppercase tracking-widest text-[var(--status-error-color)]">
            {{ t('card.purge_title') }}
          </h3>
        </div>
        <button 
          @click="emit('close')" 
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          <X :size="14" />
        </button>
      </div>

      <p class="text-xs text-[var(--fg-color)] leading-relaxed">
        Select a de-integration strategy for <span class="font-bold text-[var(--accent-color)]">{{ project.name }}</span>:
      </p>

      <!-- Strategy Options -->
      <div class="flex flex-col gap-2">
        <!-- Strategy 1: Database Only -->
        <div 
          @click="selectedStrategy = 'database_only'"
          class="border p-3 flex items-start gap-3 cursor-pointer transition-all"
          :class="[
            selectedStrategy === 'database_only'
              ? 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--fg-color)]'
              : 'border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] bg-black/20'
          ]"
        >
          <div class="mt-0.5 shrink-0 flex items-center justify-center">
            <div 
              class="w-3.5 h-3.5 border flex items-center justify-center transition-colors"
              :class="selectedStrategy === 'database_only' ? 'border-[var(--status-warn-color)] bg-[var(--status-warn-color)]' : 'border-[var(--border-color)]'"
            >
              <div v-if="selectedStrategy === 'database_only'" class="w-1.5 h-1.5 bg-[var(--bg-color)]" />
            </div>
          </div>

          <div class="flex-1 min-w-0 flex flex-col gap-1">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <Database :size="13" :class="selectedStrategy === 'database_only' ? 'text-[var(--status-warn-color)]' : 'text-[var(--dim-color)]'" />
                <span class="text-xs font-bold tracking-wide" :class="selectedStrategy === 'database_only' ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]'">
                  {{ t('card.unregister_title') }}
                </span>
              </div>
              <span class="text-[9px] px-1.5 py-0.2 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] uppercase tracking-wider font-mono">
                KEEP CONFIG
              </span>
            </div>
            <p class="text-[10px] leading-relaxed opacity-80">
              {{ t('card.unregister_desc') }}
            </p>
          </div>
        </div>

        <!-- Strategy 2: Space Him (Full De-integration) -->
        <div 
          @click="selectedStrategy = 'space_him'"
          class="border p-3 flex items-start gap-3 cursor-pointer transition-all"
          :class="[
            selectedStrategy === 'space_him'
              ? 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--fg-color)]'
              : 'border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] bg-black/20'
          ]"
        >
          <div class="mt-0.5 shrink-0 flex items-center justify-center">
            <div 
              class="w-3.5 h-3.5 border flex items-center justify-center transition-colors"
              :class="selectedStrategy === 'space_him' ? 'border-[var(--status-error-color)] bg-[var(--status-error-color)]' : 'border-[var(--border-color)]'"
            >
              <div v-if="selectedStrategy === 'space_him'" class="w-1.5 h-1.5 bg-[var(--bg-color)]" />
            </div>
          </div>

          <div class="flex-1 min-w-0 flex flex-col gap-1">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <Rocket :size="13" :class="selectedStrategy === 'space_him' ? 'text-[var(--status-error-color)]' : 'text-[var(--dim-color)]'" />
                <span class="text-xs font-bold tracking-wide" :class="selectedStrategy === 'space_him' ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]'">
                  {{ t('card.space_him_title') }}
                </span>
              </div>
              <span class="text-[9px] px-1.5 py-0.2 border border-[var(--status-error-border)] text-[var(--status-error-color)] uppercase tracking-wider font-mono font-bold">
                SURGICAL PURGE
              </span>
            </div>
            <p class="text-[10px] leading-relaxed opacity-80">
              {{ t('card.space_him_desc') }}
            </p>
          </div>
        </div>
      </div>

      <!-- Confirmation Name Input -->
      <div class="flex flex-col gap-1.5 mt-1 border-t border-[var(--border-color)]/60 pt-3">
        <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
          {{ t('card.type_to_confirm') }}
        </label>
        <AppInput 
          v-model="deleteConfirmText" 
          :placeholder="project?.name"
          @keydown.enter="handleConfirm"
        />
      </div>

      <!-- Modal Footer -->
      <div class="flex justify-end gap-3 mt-2">
        <button 
          @click="emit('close')" 
          class="px-4 py-1.5 text-xs font-bold uppercase border border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          {{ t('card.cancel') }}
        </button>
        <button 
          @click="handleConfirm" 
          :disabled="deleteConfirmText !== project.name"
          class="px-4 py-1.5 text-xs font-bold uppercase border transition-colors flex items-center gap-1.5"
          :class="[
            deleteConfirmText === project.name
              ? (selectedStrategy === 'space_him' 
                  ? 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] hover:bg-[var(--status-error-color)] hover:text-black cursor-pointer font-bold'
                  : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] hover:bg-[var(--status-warn-color)] hover:text-black cursor-pointer font-bold')
              : 'border-[var(--border-color)] text-[var(--dim-color)] opacity-50 cursor-not-allowed'
          ]"
        >
          <Rocket v-if="selectedStrategy === 'space_him'" :size="12" />
          <span>{{ selectedStrategy === 'space_him' ? t('card.space_him_action') : t('card.purge_action') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>
