<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import { Search, X, RotateCcw, Loader2 } from '@lucide/vue';
import { AppTooltip } from '../ui';
import { useDialogueStore, type AgentConversationRecord } from '../../stores/dialogueStore';
import { formatDate } from '../../utils/format';

const props = defineProps<{
  isOpen: boolean;
  agentId: string;
  projectPath?: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const store = useDialogueStore();
const query = ref('');
const results = ref<AgentConversationRecord[]>([]);
const hasSearched = ref(false);
const errorMessage = ref<string | null>(null);
const isSearching = ref(false);
const reopeningId = ref<string | null>(null);
const searchInputRef = ref<HTMLInputElement | null>(null);

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.isOpen) {
    e.preventDefault();
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});

async function loadConversations() {
  if (!props.agentId) return;
  isSearching.value = true;
  errorMessage.value = null;
  try {
    results.value = await store.searchConversations(query.value.trim(), undefined, props.projectPath, props.agentId);
    hasSearched.value = query.value.trim().length > 0;
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSearching.value = false;
  }
}

watch(
  () => props.isOpen,
  (open) => {
    if (open) {
      loadConversations().catch(() => undefined);
      setTimeout(() => searchInputRef.value?.focus(), 20);
      return;
    }
    query.value = '';
    results.value = [];
    hasSearched.value = false;
    errorMessage.value = null;
  }
);

async function runSearch() {
  await loadConversations();
}

function clearQuery() {
  query.value = '';
  loadConversations().catch(() => undefined);
}

async function reopen(conversationId: string) {
  reopeningId.value = conversationId;
  errorMessage.value = null;
  try {
    await store.reopenConversation(conversationId, props.agentId, props.projectPath);
    emit('close');
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    reopeningId.value = null;
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="isOpen"
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
      @click.self="emit('close')"
    >
      <div class="ui-surface-base border border-[var(--border-color)] max-w-xl w-full flex flex-col shadow-2xl overflow-hidden">
        <!-- Header -->
        <div class="h-9 px-3 border-b border-[var(--border-color)] ui-surface-title flex justify-between items-center shrink-0">
          <div class="flex items-center gap-2">
            <Search class="text-[var(--accent-color)] shrink-0" :size="14" />
            <h3 class="text-[10px] font-bold uppercase tracking-widest text-[var(--fg-color)]">
              Conversations · {{ agentId }}
            </h3>
          </div>
          <AppTooltip text="Close (Esc)">
            <button
              @click="emit('close')"
              class="ui-icon-btn"
            >
              <X :size="12" />
            </button>
          </AppTooltip>
        </div>

        <!-- Search Bar -->
        <form class="p-3 border-b border-[var(--border-color)] flex items-center gap-2 shrink-0" @submit.prevent="runSearch().catch(() => undefined)">
          <div class="ui-search-box flex-1 h-7 px-2">
            <Search :size="12" class="ui-search-box__icon" />
            <input
              ref="searchInputRef"
              v-model="query"
              type="text"
              placeholder="Filter by topic, decision, tag…"
              class="flex-1 bg-transparent text-[10px] text-[var(--fg-color)] placeholder:text-[var(--dim-color)] focus:outline-none min-w-0"
            />
            <button
              v-if="query"
              type="button"
              @click="clearQuery"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
            >
              <X :size="11" />
            </button>
          </div>
          <AppTooltip text="Search conversations">
            <button
              type="submit"
              :disabled="isSearching"
              class="h-7 px-2.5 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer disabled:opacity-50 flex items-center gap-1 text-[9px] font-bold uppercase"
            >
              <Loader2 v-if="isSearching" :size="11" class="animate-spin" />
              <Search v-else :size="11" />
              <span>Search</span>
            </button>
          </AppTooltip>
        </form>

        <!-- Error Banner -->
        <div
          v-if="errorMessage"
          class="mx-3 mt-3 p-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[9px] select-text"
        >
          {{ errorMessage }}
        </div>

        <!-- Results / History List -->
        <div class="p-3 flex flex-col gap-2 max-h-80 overflow-y-auto ui-surface-list">
          <div v-if="isSearching && results.length === 0" class="py-6 flex items-center justify-center gap-2 text-[10px] text-[var(--dim-color)]">
            <Loader2 :size="12" class="animate-spin text-[var(--accent-color)]" />
            <span>Loading conversations…</span>
          </div>
          <div v-else-if="results.length === 0" class="py-6 text-center text-[10px] text-[var(--dim-color)]">
            {{ hasSearched ? 'No matching conversations found.' : 'No recorded conversations for this agent yet.' }}
          </div>
          <div
            v-for="conversation in results"
            :key="conversation.id"
            class="flex items-start gap-3 p-2.5 border ui-surface-card transition-colors"
            :class="conversation.status === 'active' ? 'border-[var(--accent-color)]/60' : 'border-[var(--border-color)]'"
          >
            <div class="flex-1 min-w-0 flex flex-col gap-1">
              <div class="flex items-center gap-2 text-[10px]">
                <span class="font-bold text-[var(--fg-color)] truncate">{{ conversation.topic }}</span>
                <span
                  class="text-[8px] uppercase px-1 border shrink-0"
                  :class="conversation.status === 'active'
                    ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                    : 'border-[var(--border-color)] text-[var(--dim-color)]'"
                >
                  {{ conversation.status }}
                </span>
                <span class="text-[8px] text-[var(--dim-color)] shrink-0 ml-auto">{{ formatDate(conversation.updated_at) }}</span>
              </div>
              <p v-if="conversation.summary" class="text-[9px] text-[var(--dim-color)] leading-relaxed line-clamp-3 select-text">
                {{ conversation.summary }}
              </p>
            </div>
            <AppTooltip v-if="conversation.status !== 'active'" text="Reopen: the agent continues this conversation from a Brief">
              <button
                :disabled="reopeningId !== null"
                class="ui-icon-btn hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] disabled:opacity-50"
                @click="reopen(conversation.id)"
              >
                <RotateCcw :size="11" />
              </button>
            </AppTooltip>
          </div>
        </div>

        <!-- Footer -->
        <div class="ui-modal-footer">
          <span>{{ results.length }} {{ results.length === 1 ? 'conversation' : 'conversations' }}</span>
          <div class="flex items-center gap-1.5">
            <span class="ui-kbd">ESC</span>
            <span>Close</span>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
