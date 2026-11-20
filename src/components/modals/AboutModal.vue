<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { X, Cpu, Server, Terminal } from '@lucide/vue';
import { useCoreStore } from '../../stores/coreStore';

const props = defineProps<{
  show: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const coreStore = useCoreStore();
const appVersion = '0.2.2';

function handleKeydown(e: KeyboardEvent) {
  if (props.show && e.key === 'Escape') {
    e.stopImmediatePropagation();
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown, { capture: true });
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown, { capture: true });
});
</script>

<template>
  <div
    v-if="show"
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
    @click.self="emit('close')"
  >
    <div class="ui-surface-base border border-[var(--border-color)] max-w-md w-full flex flex-col shadow-2xl overflow-hidden">
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-[var(--border-color)] ui-surface-title">
        <div class="flex items-center gap-2.5">
          <span class="font-title text-base tracking-wider text-[var(--accent-color)]">ONE-SHIP</span>
          <span class="ui-kbd">v{{ appVersion }}</span>
        </div>
        <div class="flex items-center gap-2">
          <kbd class="ui-kbd">ESC</kbd>
          <button
            type="button"
            @click="emit('close')"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Body -->
      <div class="p-4 flex flex-col gap-3 text-xs text-[var(--fg-color)]">
        <p class="text-[11px] text-[var(--dim-color)] leading-relaxed">
          Project orchestrator, agent supervisor, and diagnostic workbench built for single-machine precision on Omarchy Linux.
        </p>

        <div class="border border-[var(--border-color)] ui-surface-card divide-y divide-[var(--border-color)]/50 text-[10px]">
          <div class="flex items-center justify-between px-3 py-2">
            <span class="flex items-center gap-2 text-[var(--dim-color)]">
              <Server :size="12" class="text-[var(--accent-color)]" />
              <span>Daemon Status</span>
            </span>
            <span
              class="uppercase font-bold"
              :class="coreStore.daemonStatus.state === 'connected' ? 'text-[var(--status-ok-color)]' : 'text-[var(--status-warn-color)]'"
            >
              {{ coreStore.daemonStatus.state }}
            </span>
          </div>

          <div class="flex items-center justify-between px-3 py-2">
            <span class="flex items-center gap-2 text-[var(--dim-color)]">
              <Cpu :size="12" class="text-[var(--accent-color)]" />
              <span>Runtime Stack</span>
            </span>
            <span class="text-[var(--fg-color)]">Tauri + Rust + Vue 3</span>
          </div>

          <div class="flex items-center justify-between px-3 py-2">
            <span class="flex items-center gap-2 text-[var(--dim-color)]">
              <Terminal :size="12" class="text-[var(--accent-color)]" />
              <span>Tracked Projects</span>
            </span>
            <span class="text-[var(--fg-color)]">{{ coreStore.projects.length }}</span>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="ui-modal-footer">
        <span>kirulab // pre-1.0 build</span>
        <span>Press <kbd class="ui-kbd">ESC</kbd> to close</span>
      </div>
    </div>
  </div>
</template>
