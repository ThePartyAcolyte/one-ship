<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { daemonCall } from '../../utils/daemonClient';
import { ArrowRightLeft, X, Check, StickyNote } from '@lucide/vue';
import { AppSelect, AppTextarea } from '../ui';

/** Brief built by the backend (Spec 30 §5); mirrors `brief::Brief`. */
interface ConversationBrief {
  text: string;
  dropped_sections: string[];
  turns_included_count: number;
}

const props = defineProps<{
  isOpen: boolean;
  sourceAgentId: string;
  projectPath: string;
  projectName: string;
  availableAgents: { id: string; role: string }[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'transfer', payload: { targetAgentId: string; handoffMessage: string }): void;
}>();

const targetAgentId = ref('');
const additionalNotes = ref('');
const brief = ref<ConversationBrief | null>(null);
const briefError = ref<string | null>(null);

const agentOptions = computed(() => {
  return props.availableAgents.map(a => ({
    label: `${a.id.toUpperCase()} - ${a.role || 'Agent'}`,
    value: a.id,
  }));
});

watch(
  () => props.isOpen,
  async (open) => {
    brief.value = null;
    briefError.value = null;
    if (!open) return;
    try {
      brief.value = await daemonCall('build_conversation_brief', {
        projectPath: props.projectPath,
        agentId: props.sourceAgentId,
      });
    } catch (err) {
      briefError.value = String(err);
    }
  },
  { immediate: true }
);

watch(
  () => [props.isOpen, props.availableAgents],
  () => {
    if (props.isOpen && props.availableAgents.length > 0) {
      // Pick first agent that is not the source agent, or the first available
      const candidate = props.availableAgents.find(a => a.id.toLowerCase() !== props.sourceAgentId.toLowerCase());
      targetAgentId.value = candidate ? candidate.id : (props.availableAgents[0]?.id || '');
    }
  },
  { immediate: true }
);

const handoffMessage = computed(() => {
  if (!brief.value) return '';
  const notes = additionalNotes.value.trim();
  return notes ? `${brief.value.text}\n## Instructions\n${notes}\n` : brief.value.text;
});

function handleTransfer() {
  if (!targetAgentId.value || !handoffMessage.value) return;
  emit('transfer', {
    targetAgentId: targetAgentId.value,
    handoffMessage: handoffMessage.value,
  });
}
</script>

<template>
  <Teleport to="body">
    <div 
      v-if="isOpen" 
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
      @click.self="emit('close')"
    >
      <div class="ui-surface-base border border-[var(--border-color)] p-5 max-w-xl w-full flex flex-col gap-4 shadow-2xl">
      <!-- Modal Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-3">
        <div class="flex items-center gap-2">
          <ArrowRightLeft class="text-[var(--accent-color)] shrink-0" :size="16" />
          <h3 class="text-xs font-bold uppercase tracking-widest text-[var(--fg-color)]">
            Context & Minutes Handoff
          </h3>
        </div>
        <button 
          @click="emit('close')" 
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          <X :size="14" />
        </button>
      </div>

      <!-- Body -->
      <div class="flex flex-col gap-3 text-xs">
        <!-- Target Agent Selector -->
        <div class="flex flex-col gap-1">
          <label class="text-[9px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Destination Agent:
          </label>
          <AppSelect 
            v-model="targetAgentId" 
            :options="agentOptions" 
            placeholder="Select target agent..."
          />
        </div>

        <!-- Additional Notes -->
        <div class="flex flex-col gap-1">
          <label class="text-[9px] uppercase tracking-wider text-[var(--dim-color)] font-bold flex items-center gap-1">
            <StickyNote :size="11" />
            <span>Additional User Instructions (Optional):</span>
          </label>
          <AppTextarea 
            v-model="additionalNotes" 
            :rows="2" 
            placeholder="E.g., continue refactoring the database connection according to the latest plan..."
          />
        </div>

        <!-- Preview Payload -->
        <div class="flex flex-col gap-1">
          <span class="text-[9px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Handoff Payload Preview:
          </span>
          <div
            v-if="briefError"
            class="p-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[9px] font-mono select-text"
          >
            {{ briefError }}
          </div>
          <div
            v-else
            class="p-2 ui-surface-list border border-[var(--border-color)]/70 text-[9px] text-[var(--fg-color)] font-mono max-h-36 overflow-y-auto whitespace-pre-wrap select-text leading-relaxed"
          >
            {{ handoffMessage || 'Building brief…' }}
          </div>
          <span v-if="brief && brief.dropped_sections.length" class="text-[9px] text-[var(--status-warn-color)] font-mono">
            Trimmed to fit: {{ brief.dropped_sections.join(', ') }}
          </span>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="flex justify-end items-center gap-2 pt-3 border-t border-[var(--border-color)]">
        <button 
          @click="emit('close')" 
          class="px-3 py-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--fg-color)] transition-colors cursor-pointer text-[10px]"
        >
          Cancel
        </button>
        <button 
          @click="handleTransfer" 
          :disabled="!targetAgentId || !brief"
          class="px-3 py-1.5 border border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black font-bold transition-all cursor-pointer text-[10px] flex items-center gap-1.5 disabled:opacity-50"
        >
          <Check :size="12" />
          <span>Execute Handoff &amp; Switch</span>
        </button>
      </div>
    </div>
    </div>
  </Teleport>
</template>
