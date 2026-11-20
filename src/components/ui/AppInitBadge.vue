<script setup lang="ts">
import { computed } from 'vue';
import { AppTooltip } from './';
import { AlertTriangle } from '@lucide/vue';

import type { ProjectInitFlagRecord } from '../../types';

export type ProjectInitFlag = ProjectInitFlagRecord;

const props = defineProps<{
  flag: ProjectInitFlag;
}>();

const emit = defineEmits<{
  (e: 'resolve', flag: ProjectInitFlag): void;
  (e: 'dismiss', flag: ProjectInitFlag): void;
}>();

const badgeStyle = computed(() => {
  switch (props.flag.flag_type) {
    case 'docs':
      return {
        border: 'border-[var(--status-warn-border)]',
        text: 'text-[var(--status-warn-color)]',
        bg: 'bg-[var(--status-warn-bg)]',
        label: '[! INIT: DOCS]',
      };
    case 'todos':
      return {
        border: 'border-[var(--status-info-border)]',
        text: 'text-[var(--status-info-color)]',
        bg: 'bg-[var(--status-info-bg)]',
        label: '[! INIT: TODOS]',
      };
    case 'tech_debt':
      return {
        border: 'border-[var(--status-warn-border)]',
        text: 'text-[var(--status-warn-color)]',
        bg: 'bg-[var(--status-warn-bg)]',
        label: '[! INIT: TECH_DEBT]',
      };
    case 'profiler':
      return {
        border: 'border-[var(--status-error-border)]',
        text: 'text-[var(--status-error-color)]',
        bg: 'bg-[var(--status-error-bg)]',
        label: '[! INIT: NO PROFILER]',
      };
    case 'invalid_config':
      return {
        border: 'border-[var(--accent-color)]',
        text: 'text-[var(--accent-color)]',
        bg: 'bg-black/30',
        label: '[! CONFIG]',
      };
    case 'removal':
      return {
        border: 'border-[var(--accent-color)]',
        text: 'text-[var(--accent-color)]',
        bg: 'bg-black/30',
        label: '[! REMOVAL]',
      };
    default:
      return {
        border: 'border-[var(--border-color)]',
        text: 'text-[var(--fg-color)]',
        bg: 'bg-black/30',
        label: `[! INIT: ${props.flag.flag_type.toUpperCase()}]`,
      };
  }
});

const tooltipText = computed(() => {
  const parts = [props.flag.title];
  if (props.flag.description) {
    parts.push(props.flag.description);
  }
  return parts.join(' — ');
});
</script>

<template>
  <div
    class="inline-flex items-center gap-1.5 px-2 py-0.5 border font-mono text-[10px] tracking-wide rounded-none select-none transition-colors"
    :class="[badgeStyle.border, badgeStyle.text, badgeStyle.bg]"
  >
    <AppTooltip :text="tooltipText" position="top">
      <div class="flex items-center gap-1 cursor-help">
        <AlertTriangle :size="11" class="shrink-0" />
        <span class="font-bold">{{ badgeStyle.label }}</span>
      </div>
    </AppTooltip>

    <div class="flex items-center gap-1 ml-1 border-l border-current/30 pl-1.5">
      <AppTooltip text="Open dedicated resolution session" position="top">
        <button
          type="button"
          class="hover:underline font-bold px-1 uppercase tracking-wider transition-colors hover:text-white"
          @click.stop="emit('resolve', flag)"
        >
          RESOLVE
        </button>
      </AppTooltip>

      <AppTooltip text="Dismiss this flag" position="top">
        <button
          type="button"
          class="opacity-60 hover:opacity-100 px-0.5 transition-opacity"
          @click.stop="emit('dismiss', flag)"
        >
          ✕
        </button>
      </AppTooltip>
    </div>
  </div>
</template>
