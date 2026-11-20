<script setup lang="ts">
import { ref, computed } from 'vue';
import { GitBranch, ChevronDown } from '@lucide/vue';
import { AppTooltip } from '../ui';
import GitBranchModal from '../modals/GitBranchModal.vue';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
}>();

const showModal = ref(false);

const currentBranch = computed(() => props.project.git_status?.branch ?? 'detached');
</script>

<template>
  <!-- Plain branch text (Spec 13 §3.13): no boxed selector; the modal owns the actions. -->
  <div v-if="project.has_git" class="min-w-0">
    <AppTooltip text="Branches">
      <button
        type="button"
        @click="showModal = true"
        class="flex items-center gap-1 min-w-0 text-[10px] font-mono text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
      >
        <GitBranch :size="11" class="text-[var(--accent-color)] shrink-0" />
        <span class="truncate font-bold">{{ currentBranch }}</span>
        <ChevronDown :size="10" class="opacity-60 shrink-0" />
      </button>
    </AppTooltip>

    <GitBranchModal
      v-if="showModal"
      :project="project"
      @close="showModal = false"
    />
  </div>
</template>
