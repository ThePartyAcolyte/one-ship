<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { BookOpen, X, Check, Sparkles } from '@lucide/vue';
import { AppTooltip } from '../ui';
import type { ProjectInitFlag } from '../ui/AppInitBadge.vue';
import type { ProjectRecord } from '../../types';

interface DocstyleOption {
  id: string;
  name: string;
  badge: string;
  tagline: string;
  description: string;
  pros: string[];
  cons: string[];
}

const props = defineProps<{
  show: boolean;
  project: ProjectRecord | null;
  flag: ProjectInitFlag | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'dismiss', flag: ProjectInitFlag): void;
  (e: 'resolve', payload: {
    flag: ProjectInitFlag;
    chosenOption: string;
    cleanupStrategy: 'purge' | 'annotate' | 'preserve';
  }): void;
}>();

const docstyles: DocstyleOption[] = [
  {
    id: 'sdd',
    name: 'Software Design Document (SDD)',
    badge: 'FORMAL ARCHITECTURE',
    tagline: 'Formal specifications, architecture RFCs, and ubiquitous language glossary under docs/.',
    description: 'Establishes a bi-directionally synchronized living specification layout. Code is treated as the materialization of specifications.',
    pros: ['Complete architectural clarity', 'Optimal for multi-agent coordination', 'Living ubiquitous language'],
    cons: ['Requires discipline to keep specs synchronized with code changes'],
  },
  {
    id: 'lean',
    name: 'Lean & ADR Micro-Specs',
    badge: 'FAST ITERATIVE',
    tagline: 'Lightweight Architectural Decision Records (ADRs) and minimal module summaries.',
    description: 'Prioritizes rapid decision records and concise module overviews without heavy specification overhead.',
    pros: ['Ultra-low documentation overhead', 'Fast for CLI utilities & small tools', 'Clean historical decision log'],
    cons: ['Less structural detail for complex multi-tier platforms'],
  },
  {
    id: 'contract',
    name: 'Contract-First API Specs',
    badge: 'SCHEMA DRIVEN',
    tagline: 'Interface schemas (OpenAPI, JSON-RPC, IPC protocols) serve as Single Source of Truth.',
    description: 'Centers documentation around formal API contracts, message payloads, and IPC schemas under docs/contracts/.',
    pros: ['Eliminates API/IPC type drift', 'Ideal for client-server & microservices', 'Machine-verifiable schemas'],
    cons: ['Focuses primarily on network boundaries rather than internal domain algorithms'],
  },
  {
    id: 'custom',
    name: 'Custom (Build with Agent)',
    badge: 'BESPOKE DESIGN',
    tagline: 'Collaborate with the AI agent to craft a bespoke documentation standard and custom skill.',
    description: 'The agent initiates a collaborative design session to establish custom rules and writes .agents/skills/custom-docstyle/ before scaffolding.',
    pros: ['100% tailored to unique domain or legacy standards', 'Generates dedicated project skill'],
    cons: ['Requires an initial design turn with the agent before scaffolding begins'],
  },
];

const selectedOptionId = ref<string>('sdd');
const cleanupStrategy = ref<'purge' | 'annotate' | 'preserve'>('preserve');

// Normalize recommended option
const recommendedId = computed(() => {
  if (!props.flag?.recommended_option) return 'sdd';
  const rec = props.flag.recommended_option.toLowerCase();
  if (rec.includes('sdd') || rec.includes('spec')) return 'sdd';
  if (rec.includes('lean') || rec.includes('fast') || rec.includes('adr')) return 'lean';
  if (rec.includes('contract') || rec.includes('api')) return 'contract';
  if (rec.includes('custom')) return 'custom';
  return 'sdd';
});

watch(() => props.show, (isOpen) => {
  if (isOpen) {
    selectedOptionId.value = recommendedId.value;
    cleanupStrategy.value = 'preserve';
  }
});

function handleStartResolution() {
  if (!props.flag) return;
  emit('resolve', {
    flag: props.flag,
    chosenOption: selectedOptionId.value,
    cleanupStrategy: cleanupStrategy.value,
  });
}
</script>

<template>
  <div v-if="show && flag" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4">
    <div class="ui-surface-modal border border-[var(--border-color)] shadow-2xl max-w-4xl w-full flex flex-col max-h-[90vh] overflow-hidden font-mono text-xs">
      
      <!-- Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] px-5 py-3 shrink-0">
        <div class="flex items-center gap-3">
          <div class="w-7 h-7 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] flex items-center justify-center font-bold">
            <BookOpen :size="15" />
          </div>
          <div>
            <h3 class="font-bold uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-2">
              <span>Resolve Documentation Standard</span>
              <span class="text-[9px] px-2 py-0.5 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)] uppercase">
                [! INIT: DOCS]
              </span>
            </h3>
            <p class="text-[10px] text-[var(--dim-color)]">
              Target Project: <span class="text-[var(--accent-color)]">{{ project?.name }}</span>
            </p>
          </div>
        </div>

        <AppTooltip text="Close" position="left">
          <button 
            @click="emit('close')"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors p-1 cursor-pointer"
          >
            <X :size="16" />
          </button>
        </AppTooltip>
      </div>

      <!-- Body -->
      <div class="p-5 flex-1 overflow-y-auto flex flex-col gap-5">
        
        <!-- Context Banner -->
        <div class="p-3 border border-[var(--border-color)] bg-black/30 flex flex-col gap-1">
          <div class="text-[10px] uppercase font-bold text-[var(--dim-color)] tracking-wider">
            Reconnaissance Finding
          </div>
          <p class="text-[11px] text-[var(--fg-color)] leading-relaxed">
            {{ flag.description || 'No standardized living documentation architecture detected in project root.' }}
          </p>
        </div>

        <!-- Archetypes Grid -->
        <div class="flex flex-col gap-2">
          <div class="text-[10px] uppercase font-bold text-[var(--dim-color)] tracking-wider">
            Select Living Documentation Archetype (Injected Skill)
          </div>

          <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div
              v-for="opt in docstyles"
              :key="opt.id"
              @click="selectedOptionId = opt.id"
              class="border p-3 flex flex-col justify-between gap-3 cursor-pointer transition-colors relative select-none"
              :class="selectedOptionId === opt.id 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/5 ring-1 ring-[var(--accent-color)]' 
                : 'border-[var(--border-color)] bg-black/20 hover:border-[var(--dim-color)] hover:bg-black/30'"
            >
              <!-- Card Header -->
              <div class="flex flex-col gap-1">
                <div class="flex justify-between items-start gap-2">
                  <span class="font-bold text-xs text-[var(--fg-color)]">{{ opt.name }}</span>
                  <div class="flex items-center gap-1.5 shrink-0">
                    <span 
                      v-if="recommendedId === opt.id"
                      class="text-[9px] px-1.5 py-0.2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)] uppercase flex items-center gap-1"
                    >
                      <Sparkles :size="9" /> RECOMMENDED
                    </span>
                    <span 
                      class="w-4 h-4 border flex items-center justify-center text-[10px]"
                      :class="selectedOptionId === opt.id ? 'border-[var(--accent-color)] bg-[var(--accent-color)] text-black' : 'border-[var(--border-color)]'"
                    >
                      <Check v-if="selectedOptionId === opt.id" :size="12" />
                    </span>
                  </div>
                </div>

                <p class="text-[11px] text-[var(--dim-color)] leading-relaxed">
                  {{ opt.tagline }}
                </p>
              </div>

              <!-- Pros & Cons -->
              <div class="border-t border-[var(--border-color)]/60 pt-2 flex flex-col gap-1 text-[10px]">
                <div v-for="pro in opt.pros" :key="pro" class="text-[var(--status-ok-color)]/90 flex items-center gap-1">
                  <span>+</span> <span>{{ pro }}</span>
                </div>
                <div v-for="con in opt.cons" :key="con" class="text-[var(--status-error-color)]/80 flex items-center gap-1">
                  <span>-</span> <span>{{ con }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Policies & Cleanup Configuration -->
        <div class="border border-[var(--border-color)] p-3 bg-black/20 flex flex-col gap-3">
          <div class="text-[10px] uppercase font-bold text-[var(--dim-color)] tracking-wider">
            Execution Policies
          </div>

          <div class="grid grid-cols-1 gap-4">
            <!-- Cleanup Policy -->
            <div class="flex flex-col gap-1">
              <span class="font-bold text-xs text-[var(--fg-color)]">Legacy Documentation Handling</span>
              <div class="grid grid-cols-3 gap-1.5 mt-0.5">
                <button
                  type="button"
                  @click="cleanupStrategy = 'preserve'"
                  class="px-2 py-1 text-[10px] uppercase border transition-colors cursor-pointer text-center font-mono"
                  :class="cleanupStrategy === 'preserve' 
                    ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--dim-color)]'"
                >
                  Preserve
                </button>
                <button
                  type="button"
                  @click="cleanupStrategy = 'annotate'"
                  class="px-2 py-1 text-[10px] uppercase border transition-colors cursor-pointer text-center font-mono"
                  :class="cleanupStrategy === 'annotate' 
                    ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--dim-color)]'"
                >
                  Annotate
                </button>
                <button
                  type="button"
                  @click="cleanupStrategy = 'purge'"
                  class="px-2 py-1 text-[10px] uppercase border transition-colors cursor-pointer text-center font-mono"
                  :class="cleanupStrategy === 'purge' 
                    ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold' 
                    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--dim-color)]'"
                >
                  Purge
                </button>
              </div>
            </div>
          </div>
        </div>

      </div>

      <!-- Footer Actions -->
      <div class="flex justify-between items-center px-5 py-3 border-t border-[var(--border-color)] bg-black/30 shrink-0">
        <button
          type="button"
          @click="emit('dismiss', flag)"
          class="px-3 py-1.5 text-xs uppercase border border-[var(--status-error-border)] text-[var(--status-error-color)] hover:bg-[var(--status-error-bg)] transition-colors cursor-pointer font-bold"
        >
          Dismiss Flag
        </button>

        <div class="flex gap-3">
          <button 
            type="button"
            @click="emit('close')"
            class="px-4 py-1.5 text-xs font-bold uppercase border border-[var(--border-color)] hover:bg-[var(--fg-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer"
          >
            Cancel
          </button>
          <button 
            type="button"
            @click="handleStartResolution"
            class="px-5 py-1.5 text-xs font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-1.5"
          >
            <span>Start Resolution Session</span>
          </button>
        </div>
      </div>

    </div>
  </div>
</template>
