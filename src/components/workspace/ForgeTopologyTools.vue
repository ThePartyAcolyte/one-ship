<script setup lang="ts">
import { computed } from 'vue';
import { GitFork } from '@lucide/vue';
import { useForgeCollabStore } from '../../stores/forgeCollabStore';

const store = useForgeCollabStore();

const ctx = computed(() => store.collabContext);
const providerLabel = computed(() => ctx.value?.providerLabel ?? 'Forge');
</script>

<template>
  <!-- Forge repository topology shown in the Project panel bar (Spec 34 §6.5). -->
  <div class="flex items-center gap-2 text-[11px] font-mono min-w-0 whitespace-nowrap">
    <span
      class="text-[9px] px-1.5 py-0.5 border border-[var(--accent-color)]/50 bg-[var(--accent-color)]/10 text-[var(--accent-color)] uppercase font-bold"
    >
      {{ providerLabel }}
    </span>

    <div class="flex items-center gap-1.5 px-2 py-0.5 border border-[var(--border-color)] bg-[var(--bg-color)]/60">
      <GitFork :size="12" class="text-[var(--accent-color)] shrink-0" />
      <span class="text-[var(--dim-color)]">origin:</span>
      <span class="text-[var(--fg-color)] font-bold">{{ ctx?.originRepo || 'none' }}</span>
      <span
        v-if="ctx?.isFork"
        class="text-[9px] px-1 border border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)] uppercase font-bold"
      >
        FORK
      </span>
    </div>

    <div v-if="ctx?.hasUpstream" class="flex items-center gap-1.5 px-2 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)]/40">
      <span class="text-[var(--dim-color)]">upstream:</span>
      <span class="text-[var(--fg-color)] font-bold">{{ ctx.upstreamRepo }}</span>
      <span
        v-if="ctx.behindUpstreamCount > 0"
        class="text-[9px] text-[var(--status-warn-color)] font-bold"
      >
        -{{ ctx.behindUpstreamCount }} behind
      </span>
    </div>

    <div
      v-else-if="ctx?.isFork && ctx?.parentRepo"
      class="flex items-center gap-1.5 px-2 py-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]"
    >
      <span>parent: {{ ctx.parentRepo }}</span>
    </div>

    <div
      v-if="ctx?.targetRepo"
      class="text-[10px] text-[var(--dim-color)] flex items-center gap-1"
    >
      <span>target:</span>
      <span class="text-[var(--fg-color)] font-bold">{{ ctx.targetRepo }}</span>
      <span v-if="ctx.defaultBranch">({{ ctx.defaultBranch }})</span>
    </div>
  </div>
</template>
