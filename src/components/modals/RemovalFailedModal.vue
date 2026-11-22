<script setup lang="ts">
import { Bot, X } from '@lucide/vue';
import { AppTooltip } from '../ui';

export interface RemovalFailure {
  project: { id: number; name: string; path: string };
  purgeTraces: boolean;
  report: string;
}

defineProps<{
  failure: RemovalFailure | null;
}>();

const emit = defineEmits<{
  (e: 'cancel'): void;
  (e: 'resolve-with-agent'): void;
}>();
</script>

<template>
  <div
    v-if="failure"
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
    @click.self="emit('cancel')"
  >
    <div class="ui-surface-modal border border-[var(--border-color)] p-6 max-w-lg w-full flex flex-col gap-4 shadow-2xl">
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-3">
        <h3 class="text-xs font-bold uppercase tracking-widest text-[var(--fg-color)]">
          Removal failed: {{ failure.project.name }}
        </h3>
        <button
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
          @click="emit('cancel')"
        >
          <X :size="14" />
        </button>
      </div>

      <pre class="text-[10px] leading-relaxed text-[var(--fg-color)] whitespace-pre-wrap break-words max-h-[40vh] overflow-y-auto border border-[var(--border-color)] bg-black/30 p-3 select-text">{{ failure.report }}</pre>

      <div class="flex justify-end gap-3">
        <AppTooltip text="Keep the project registered" position="top">
          <button
            class="px-4 py-1.5 text-xs font-bold uppercase border border-[var(--border-color)] hover:border-[var(--dim-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
            @click="emit('cancel')"
          >
            Cancel
          </button>
        </AppTooltip>
        <AppTooltip text="An agent fixes the failures and asks you to approve the deletion" position="top">
          <button
            class="px-4 py-1.5 text-xs font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer flex items-center gap-1.5"
            @click="emit('resolve-with-agent')"
          >
            <Bot :size="12" />
            <span>Resolve with agent</span>
          </button>
        </AppTooltip>
      </div>
    </div>
  </div>
</template>
