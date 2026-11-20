<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { 
  Search, X, FileCode, BookOpen, History, CheckSquare, 
  AlertOctagon, CornerDownLeft, Loader2, GitFork
} from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { useProjectExplorerStore } from '../../stores/projectExplorerStore';
import { AppInput } from '../ui';
import type { KnowledgeSearchResult } from '../../types/generated/KnowledgeSearchResult';
import type { KnowledgeRelation } from '../../types/generated/KnowledgeRelation';
import type { KnowledgeScope } from '../../types/generated/KnowledgeScope';

const props = defineProps<{
  isOpen: boolean;
  activeProjectId?: number | null;
  activeProjectPath?: string | null;
  initialQuery?: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const layoutStore = useWorkspaceLayoutStore();
const explorerStore = useProjectExplorerStore();

const query = ref('');
const selectedScope = ref<KnowledgeScope>('all');
const results = ref<KnowledgeSearchResult[]>([]);
const isSearching = ref(false);
const searchError = ref<string | null>(null);
const selectedIndex = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);

const scopes: { id: KnowledgeScope; label: string }[] = [
  { id: 'all', label: 'ALL' },
  { id: 'code', label: 'CODE' },
  { id: 'specs', label: 'SPECS' },
  { id: 'journal', label: 'JOURNAL' },
  { id: 'tasks', label: 'TASKS' },
  { id: 'tech_debt', label: 'DEBT' },
];

let debounceTimeout: ReturnType<typeof setTimeout> | null = null;

function performSearch() {
  if (debounceTimeout) {
    clearTimeout(debounceTimeout);
  }

  const trimmed = query.value.trim();
  if (!trimmed) {
    results.value = [];
    isSearching.value = false;
    searchError.value = null;
    return;
  }

  isSearching.value = true;
  searchError.value = null;

  debounceTimeout = setTimeout(async () => {
    try {
      const res = await daemonCall('search_knowledge', {
        query: trimmed,
        scope: selectedScope.value,
        limit: 15,
        projectId: props.activeProjectId ?? null,
        includeRelations: true,
      });
      const seen = new Set<string>();
      const deduped = res.filter((item) => {
        if (seen.has(item.id)) return false;
        seen.add(item.id);
        return true;
      });
      deduped.sort((a, b) => b.score - a.score);
      results.value = deduped;
      selectedIndex.value = 0;
    } catch (err: unknown) {
      console.error('[Omnisearch] Search query failed:', err);
      searchError.value = err instanceof Error ? err.message : String(err);
      results.value = [];
    } finally {
      isSearching.value = false;
    }
  }, 250);
}

watch([query, selectedScope], () => {
  performSearch();
});

watch(() => props.isOpen, (open) => {
  if (open) {
    query.value = props.initialQuery ?? '';
    results.value = [];
    searchError.value = null;
    selectedIndex.value = 0;
    nextTick(() => {
      inputRef.value?.focus();
    });
  }
});

function handleKeydown(e: KeyboardEvent) {
  if (!props.isOpen) return;

  if (e.key === 'Escape') {
    emit('close');
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    if (results.value.length > 0) {
      selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
    }
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    if (results.value.length > 0) {
      selectedIndex.value = (selectedIndex.value - 1 + results.value.length) % results.value.length;
    }
  } else if (e.key === 'Enter') {
    e.preventDefault();
    if (results.value.length > 0 && results.value[selectedIndex.value]) {
      handleSelectResult(results.value[selectedIndex.value]);
    }
  }
}

async function handleSelectResult(item: KnowledgeSearchResult) {
  emit('close');

  if (item.filePath && props.activeProjectPath) {
    if (item.filePath.startsWith('journal://')) {
      layoutStore.setWorkspaceTab('devlog');
      return;
    }

    if (item.filePath === 'TODO.md' || item.filePath === 'TECHNICAL_DEBT.md') {
      layoutStore.setWorkspaceTab('overview');
      return;
    }

    // Open file in codebase explorer
    layoutStore.setWorkspaceTab('files');
    await explorerStore.selectProjectDocument(
      props.activeProjectPath,
      item.filePath,
      props.activeProjectId ?? undefined
    );
  } else if (item.entityType === 'journal') {
    layoutStore.setWorkspaceTab('devlog');
  } else {
    layoutStore.setWorkspaceTab('overview');
  }
}

function getScopeIcon(entityType: string) {
  switch (entityType.toLowerCase()) {
    case 'code':
      return FileCode;
    case 'specs':
      return BookOpen;
    case 'journal':
      return History;
    case 'tasks':
      return CheckSquare;
    case 'tech_debt':
      return AlertOctagon;
    default:
      return Search;
  }
}

function formatScorePct(score: number): string {
  return `${Math.round(score * 100)}%`;
}

function formatRelationBadge(rel: KnowledgeRelation): string {
  const outgoing = rel.direction === 'outgoing';
  switch (rel.edgeType) {
    case 'touched':
      return outgoing ? 'TOUCHED →' : '← TOUCHED BY';
    case 'documents':
      return outgoing ? 'DOCUMENTS →' : '← DOCUMENTED BY';
    case 'governs':
      return outgoing ? 'GOVERNS →' : '← GOVERNED BY';
    case 'affects':
      return outgoing ? 'AFFECTS →' : '← AFFECTED BY';
    case 'relates_to':
      return outgoing ? 'RELATES →' : '← RELATED';
    case 'references':
      return outgoing ? 'REFS →' : '← REF BY';
    default:
      return outgoing ? `${rel.edgeType.toUpperCase()} →` : `← ${rel.edgeType.toUpperCase()}`;
  }
}

async function handleSelectRelation(rel: KnowledgeRelation) {
  emit('close');

  if (rel.peerFilePath && props.activeProjectPath) {
    if (rel.peerFilePath.startsWith('journal://')) {
      layoutStore.setWorkspaceTab('devlog');
      return;
    }
    if (rel.peerFilePath === 'TODO.md' || rel.peerFilePath === 'TECHNICAL_DEBT.md') {
      layoutStore.setWorkspaceTab('overview');
      return;
    }
    layoutStore.setWorkspaceTab('files');
    await explorerStore.selectProjectDocument(
      props.activeProjectPath,
      rel.peerFilePath,
      props.activeProjectId ?? undefined
    );
  } else if (rel.peerEntityType === 'journal') {
    layoutStore.setWorkspaceTab('devlog');
  } else {
    layoutStore.setWorkspaceTab('overview');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
  if (debounceTimeout) clearTimeout(debounceTimeout);
});
</script>

<template>
  <div 
    v-if="isOpen" 
    class="fixed inset-0 ui-overlay-backdrop flex items-start justify-center z-50 p-4 pt-16 sm:pt-24 font-mono select-none"
    @click.self="emit('close')"
  >
    <div class="bg-[var(--bg-color)] border border-[var(--border-color)] max-w-3xl w-full flex flex-col shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-100">
      
      <!-- Top Search Bar -->
      <div class="ui-search-box !border-x-0 !border-t-0 px-4 py-3 gap-3">
        <Search class="w-4 h-4 ui-search-box__icon" />
        <AppInput
          ref="inputRef"
          v-model="query"
          type="text"
          class="!bg-transparent !border-none !h-auto !px-0 !py-0"
          placeholder="Search code, specs, journals, tasks, and tech debt across knowledge space..."
        />
        <div class="flex items-center gap-2 shrink-0">
          <Loader2 v-if="isSearching" class="w-3.5 h-3.5 animate-spin text-[var(--accent-color)]" />
          <kbd class="ui-kbd">ESC</kbd>
          <button 
            @click="emit('close')"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Scope Filter Pills -->
      <div class="flex items-center gap-1.5 px-4 py-2 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 overflow-x-auto text-[10px]">
        <span class="text-[var(--dim-color)] text-[9px] uppercase tracking-wider mr-1">Scope:</span>
        <button
          v-for="s in scopes"
          :key="s.id"
          @click="selectedScope = s.id"
          class="ui-filter-pill"
          :class="selectedScope === s.id ? 'ui-filter-pill--active' : ''"
        >
          {{ s.label }}
        </button>
      </div>

      <!-- Results Body -->
      <div class="max-h-[60vh] overflow-y-auto divide-y divide-[var(--border-color)]/40">
        
        <!-- Empty Query State -->
        <div v-if="!query.trim()" class="p-8 text-center flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] text-xs">
          <Search class="w-6 h-6 text-[var(--dim-color)]/60 mb-1" />
          <p class="font-bold">Omnisearch Hybrid Knowledge Discovery</p>
          <p class="text-[10px] max-w-md leading-relaxed text-[var(--dim-color)]/80">
            BGE-M3 semantic search across architectural specs, session minutes, tasks, and tech debt paired with deterministic Tree-sitter AST symbol lookup.
          </p>
        </div>

        <!-- No Results State -->
        <div v-else-if="!isSearching && results.length === 0" class="p-8 text-center flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] text-xs">
          <p class="font-bold text-[var(--status-warn-color)]">No matching knowledge found</p>
          <p class="text-[10px]">Try refining your query or switching scope filters.</p>
        </div>

        <!-- Result Item Card -->
        <div
          v-for="(item, idx) in results"
          :key="`${item.id}-${idx}`"
          @click="handleSelectResult(item)"
          @mouseenter="selectedIndex = idx"
          class="p-3 sm:px-4 cursor-pointer transition-colors flex flex-col gap-1.5"
          :class="selectedIndex === idx ? 'bg-[var(--accent-color)]/10 border-l-2 border-l-[var(--accent-color)]' : 'hover:bg-[var(--ui-bg-card)]/40'"
        >
          <!-- Item Header: Scope badge, Title, Relevance % -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2 min-w-0">
              <component :is="getScopeIcon(item.entityType)" class="w-3.5 h-3.5 text-[var(--accent-color)] shrink-0" />
              <span class="text-[9px] px-1 py-0.2 border border-[var(--border-color)] text-[var(--dim-color)] uppercase shrink-0">
                {{ item.entityType }}
              </span>
              <span class="font-bold text-xs text-[var(--fg-color)] truncate">
                {{ item.title }}
              </span>
            </div>
            
            <div class="flex items-center gap-2 shrink-0 text-[10px]">
              <span 
                class="px-1.5 py-0.2 border text-[9px] font-bold"
                :class="item.score >= 0.8 
                  ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]' 
                  : 'border-[var(--border-color)] text-[var(--dim-color)]'"
              >
                {{ formatScorePct(item.score) }}
              </span>
              <CornerDownLeft v-if="selectedIndex === idx" class="w-3 h-3 text-[var(--accent-color)]" />
            </div>
          </div>

          <!-- File Path & Line Offset -->
          <div v-if="item.filePath" class="text-[10px] text-[var(--dim-color)] flex items-center gap-1 truncate">
            <span>{{ item.filePath }}</span>
            <span v-if="item.lineStart && item.lineStart > 0" class="text-[var(--accent-color)]">
              :L{{ item.lineStart }}{{ item.lineEnd && item.lineEnd > item.lineStart ? `-${item.lineEnd}` : '' }}
            </span>
          </div>

          <!-- Content Snippet Preview -->
          <pre class="text-[10px] text-[var(--dim-color)] bg-[var(--ui-bg-card)]/30 border border-[var(--border-color)]/30 p-2 overflow-x-hidden font-mono line-clamp-3 leading-relaxed whitespace-pre-wrap select-text">{{ item.contentPreview }}</pre>

          <!-- 1-Hop Knowledge Graph Relations -->
          <div
            v-if="item.relations && item.relations.length > 0"
            class="flex items-center gap-1.5 flex-wrap pt-0.5"
          >
            <span class="inline-flex items-center gap-1 text-[8px] uppercase tracking-wider text-[var(--dim-color)]">
              <GitFork class="w-2.5 h-2.5 text-[var(--accent-color)]" />
              Graph:
            </span>
            <button
              v-for="rel in item.relations"
              :key="`${rel.direction}-${rel.edgeType}-${rel.peerId}`"
              type="button"
              @click.stop="handleSelectRelation(rel)"
              class="inline-flex items-center gap-1 px-1.5 py-0.5 text-[9px] border border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 hover:border-[var(--accent-color)] text-[var(--fg-color)] transition-colors cursor-pointer max-w-[260px]"
            >
              <span class="text-[8px] uppercase text-[var(--accent-color)] shrink-0">{{ formatRelationBadge(rel) }}</span>
              <span class="truncate">{{ rel.peerTitle }}</span>
              <span v-if="rel.peerLineStart && rel.peerLineStart > 0" class="text-[8px] text-[var(--dim-color)] shrink-0">:L{{ rel.peerLineStart }}</span>
            </button>
          </div>
        </div>

      </div>

      <!-- Footer Info -->
      <div class="ui-modal-footer">
        <div class="flex items-center gap-3">
          <span>Navigate <kbd class="ui-kbd">↑</kbd> <kbd class="ui-kbd">↓</kbd></span>
          <span>Open <kbd class="ui-kbd">ENTER</kbd></span>
        </div>
        <span>{{ results.length }} result{{ results.length === 1 ? '' : 's' }}</span>
      </div>

    </div>
  </div>
</template>
