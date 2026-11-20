<script setup lang="ts">
import { ref, computed } from 'vue';
import { ShieldAlert, Check, X, MessageSquare, Loader2 } from '@lucide/vue';
import { useCoreStore } from '../../stores/coreStore';
import { AppTooltip, AppTextarea } from '../ui';
import { marked } from 'marked';

const coreStore = useCoreStore();

const feedbackText = ref('');
const isSubmitting = ref(false);

const pending = computed(() => coreStore.pendingApproval);
// `complete_project_removal`: approving deletes the project, rejecting cancels the removal.
const isProjectRemoval = computed(() => pending.value?.approval_type === 'project_removal');

const renderedContent = computed(() => {
  if (!pending.value?.payload_markdown) return '';
  try {
    return marked.parse(pending.value.payload_markdown) as string;
  } catch {
    return pending.value.payload_markdown;
  }
});

async function handleApprove() {
  if (!pending.value || isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    const response = feedbackText.value.trim() 
      ? `Approved: ${feedbackText.value.trim()}`
      : 'Approved';
    await coreStore.submitApprovalResponse(pending.value.task_id, response);
    feedbackText.value = '';
  } finally {
    isSubmitting.value = false;
  }
}

async function handleRequestChanges() {
  if (!pending.value || isSubmitting.value) return;
  if (!feedbackText.value.trim()) {
    return;
  }
  isSubmitting.value = true;
  try {
    await coreStore.submitApprovalResponse(pending.value.task_id, feedbackText.value.trim());
    feedbackText.value = '';
  } finally {
    isSubmitting.value = false;
  }
}

async function handleReject() {
  if (!pending.value || isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    const response = feedbackText.value.trim() 
      ? `Rejected: ${feedbackText.value.trim()}`
      : 'Rejected';
    await coreStore.submitApprovalResponse(pending.value.task_id, response);
    feedbackText.value = '';
  } finally {
    isSubmitting.value = false;
  }
}
</script>

<template>
  <div 
    v-if="pending"
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
  >
    <div class="bg-[var(--bg-color)] border border-[var(--status-warn-border)] w-full max-w-4xl max-h-[90vh] flex flex-col shadow-2xl text-[var(--fg-color)]">
      <!-- Modal Header -->
      <div class="border-b border-[var(--status-warn-border)] px-4 py-2.5 flex items-center justify-between bg-[var(--status-warn-bg)]">
        <div class="flex items-center gap-2 text-xs min-w-0">
          <ShieldAlert :size="15" class="text-[var(--status-warn-color)] shrink-0 animate-pulse" />
          <span class="font-bold tracking-wider text-[var(--status-warn-color)] uppercase">ORCHESTRATION APPROVAL REQUIRED</span>
          <span class="text-[var(--dim-color)] truncate">// {{ pending.phase }}</span>
        </div>
        <AppTooltip text="Reject / Close" position="bottom">
          <button 
            type="button"
            class="text-[var(--dim-color)] hover:text-[var(--status-error-color)] p-1 cursor-pointer transition-colors"
            :disabled="isSubmitting"
            @click="handleReject"
          >
            <X :size="16" />
          </button>
        </AppTooltip>
      </div>

      <!-- Project Path Subheader (if present) -->
      <div v-if="pending.project_path" class="px-4 py-1.5 bg-black/40 border-b border-[var(--border-color)] text-[10px] text-[var(--dim-color)] flex items-center gap-2">
        <span class="text-[var(--accent-color)] font-bold">TARGET:</span>
        <span class="truncate font-mono">{{ pending.project_path }}</span>
      </div>

      <!-- Markdown Payload Area -->
      <div class="p-4 overflow-y-auto flex-1 space-y-4 max-h-[50vh] select-text">
        <div 
          class="prose prose-invert max-w-none text-xs leading-relaxed text-[var(--fg-color)] markdown-body"
          v-html="renderedContent"
        />
      </div>

      <!-- Feedback & Action Footer -->
      <div class="border-t border-[var(--border-color)] p-4 bg-black/40 flex flex-col gap-3">
        <div>
          <label class="block text-[10px] uppercase font-bold text-[var(--dim-color)] mb-1">
            Feedback / Requested Modifications (Optional for Approval):
          </label>
          <AppTextarea 
            v-model="feedbackText"
            :rows="2"
            placeholder="Type specific modifications or instructions for the agent..."
            :disabled="isSubmitting"
          />
        </div>

        <div class="flex items-center justify-between gap-2 pt-1 flex-wrap">
          <button
            type="button"
            class="px-3 py-1.5 bg-[var(--status-error-bg)] hover:bg-[var(--status-error-color)]/20 border border-[var(--status-error-border)] hover:border-[var(--status-error-color)] text-[var(--status-error-color)] text-[10px] uppercase tracking-wider flex items-center gap-1.5 cursor-pointer transition-colors disabled:opacity-50"
            :disabled="isSubmitting"
            @click="handleReject"
          >
            <X :size="12" />
            <span>{{ isProjectRemoval ? 'Cancel' : 'Reject' }}</span>
          </button>

          <div class="flex items-center gap-2">
            <button
              type="button"
              class="px-3 py-1.5 bg-black/60 hover:bg-white/5 border border-[var(--status-warn-border)] hover:border-[var(--status-warn-color)] text-[var(--status-warn-color)] text-[10px] uppercase tracking-wider flex items-center gap-1.5 cursor-pointer transition-colors disabled:opacity-50"
              :disabled="isSubmitting || !feedbackText.trim()"
              @click="handleRequestChanges"
            >
              <MessageSquare :size="12" />
              <span>Request Changes</span>
            </button>

            <button
              type="button"
              class="px-4 py-1.5 bg-[var(--status-ok-bg)] hover:bg-[var(--status-ok-color)]/30 border border-[var(--status-ok-border)] hover:border-[var(--status-ok-color)] text-[var(--status-ok-color)] text-[10px] font-bold uppercase tracking-wider flex items-center gap-1.5 cursor-pointer transition-colors disabled:opacity-50"
              :disabled="isSubmitting"
              @click="handleApprove"
            >
              <Loader2 v-if="isSubmitting" :size="12" class="animate-spin" />
              <Check v-else :size="12" />
              <span>{{ isProjectRemoval ? 'Delete' : 'Approve Phase' }}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4) {
  font-weight: bold;
  color: var(--fg-color);
  margin-top: 0.75rem;
  margin-bottom: 0.25rem;
}
.markdown-body :deep(p) {
  margin-bottom: 0.5rem;
}
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 1.25rem;
  margin-bottom: 0.5rem;
}
.markdown-body :deep(li) {
  list-style-type: disc;
  margin-bottom: 0.2rem;
}
.markdown-body :deep(code) {
  background: rgba(255, 255, 255, 0.08);
  padding: 0.1rem 0.3rem;
  border-radius: 2px;
  font-size: 0.9em;
}
.markdown-body :deep(pre) {
  background: rgba(0, 0, 0, 0.5);
  border: 1px solid var(--border-color);
  padding: 0.5rem;
  overflow-x: auto;
  margin-bottom: 0.5rem;
}
.markdown-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin-bottom: 0.5rem;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--border-color);
  padding: 0.25rem 0.5rem;
  text-align: left;
}
.markdown-body :deep(th) {
  background: rgba(255, 255, 255, 0.05);
}
</style>
