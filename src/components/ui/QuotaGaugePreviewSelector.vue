<script setup lang="ts">
import { computed } from 'vue';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import AppQuotaConcentric from './AppQuotaConcentric.vue';
import AppQuotaBar from './AppQuotaBar.vue';
import AppQuotaPie from './AppQuotaPie.vue';
import type { QuotaGaugeStyle } from './quotaUtils';

const props = defineProps<{
  modelValue?: QuotaGaugeStyle;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: QuotaGaugeStyle): void;
}>();

const metricsStore = useAgentMetricsStore();

const selectedStyle = computed({
  get: () => props.modelValue || metricsStore.quotaGaugeStyle || 'concentric',
  set: (val: QuotaGaugeStyle) => {
    emit('update:modelValue', val);
    metricsStore.setQuotaGaugeStyle(val);
  },
});

interface StyleOption {
  id: QuotaGaugeStyle;
  name: string;
  tagline: string;
  description: string;
}

const styleOptions: StyleOption[] = [
  {
    id: 'concentric',
    name: 'Concentric Dual-Ring',
    tagline: 'RADAR CLOCK',
    description: 'Outer ring tracks rolling window countdown; inner disc shows quota percentage.',
  },
  {
    id: 'bars',
    name: 'Dual-Linear Bars',
    tagline: 'BI-METRIC BARS',
    description: 'Top brutalist bar displays capacity; bottom hairline sub-bar tracks time decay.',
  },
  {
    id: 'classic',
    name: 'Classic Micro Pie',
    tagline: 'MINIMALIST',
    description: 'Classic solid pie slice indicating remaining quota percentage.',
  },
];

// Sample mock data for live rendering previews
const mockStreams = [
  { category: 'gemini-5h' as const, label: 'GEM 5H', pct: 78, resetsAt: '2h 15m' },
  { category: 'gemini-7d' as const, label: 'GEM 7D', pct: 92, resetsAt: '5d 04h' },
  { category: 'other-5h' as const, label: '3P 5H', pct: 40, resetsAt: '1h 10m' },
  { category: 'other-7d' as const, label: '3P 7D', pct: 68, resetsAt: '3d 18h' },
];

function selectOption(id: QuotaGaugeStyle) {
  selectedStyle.value = id;
}
</script>

<template>
  <div class="flex flex-col gap-2.5 font-mono">
    <div 
      v-for="opt in styleOptions" 
      :key="opt.id"
      @click="selectOption(opt.id)"
      class="border transition-all cursor-pointer p-2.5 bg-[var(--ui-bg-card)]/50 hover:bg-[var(--ui-bg-card)] relative select-none"
      :class="selectedStyle === opt.id 
        ? 'border-[var(--accent-color)] ring-1 ring-[var(--accent-color)]/40 bg-[var(--accent-color)]/5' 
        : 'border-[var(--border-color)] hover:border-[var(--dim-color)]'"
    >
      <!-- Header Row: Radio + Title + Tagline -->
      <div class="flex items-center justify-between gap-2 mb-1.5">
        <div class="flex items-center gap-2">
          <div 
            class="w-3.5 h-3.5 rounded-full border flex items-center justify-center shrink-0 transition-colors"
            :class="selectedStyle === opt.id 
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]' 
              : 'border-[var(--dim-color)] bg-transparent'"
          >
            <div 
              v-if="selectedStyle === opt.id" 
              class="w-1.5 h-1.5 rounded-full bg-black"
            />
          </div>
          <span class="text-[11px] font-bold tracking-tight text-[var(--fg-color)] uppercase">
            {{ opt.name }}
          </span>
          <span class="text-[9px] px-1 py-0.2 bg-white/5 border border-[var(--border-color)] text-[var(--dim-color)]">
            {{ opt.tagline }}
          </span>
        </div>

        <span 
          v-if="selectedStyle === opt.id" 
          class="text-[9px] font-bold text-[var(--accent-color)] tracking-wider"
        >
          [ACTIVE]
        </span>
      </div>

      <!-- Description -->
      <p class="text-[10px] text-[var(--dim-color)] mb-2 pl-5 leading-tight">
        {{ opt.description }}
      </p>

      <!-- Live Rendered Preview Box -->
      <div class="ml-5 p-2 bg-[var(--bg-color)] border border-[var(--border-color)]/80 flex flex-col gap-1.5">
        <div class="text-[8px] uppercase tracking-widest text-[var(--dim-color)] flex justify-between">
          <span>Live Rendered Preview</span>
          <span>4-Stream Quota Telemetry</span>
        </div>

        <!-- Rendered sample gauges -->
        <div class="flex items-center justify-between gap-2 sm:gap-4 pt-1">
          <div 
            v-for="stream in mockStreams" 
            :key="stream.category"
            class="flex items-center gap-1.5 min-w-0"
          >
            <!-- Render according to this card's style -->
            <AppQuotaConcentric 
              v-if="opt.id === 'concentric'"
              :category="stream.category"
              :percentage="stream.pct"
              :resetsAt="stream.resetsAt"
              :size="20"
            />
            <AppQuotaBar 
              v-else-if="opt.id === 'bars'"
              :category="stream.category"
              :percentage="stream.pct"
              :resetsAt="stream.resetsAt"
              :width="32"
            />
            <AppQuotaPie 
              v-else
              :category="stream.category"
              :percentage="stream.pct"
              :resetsAt="stream.resetsAt"
              :size="15"
            />

            <!-- Subtle metric label -->
            <div class="flex flex-col text-[8px] font-mono leading-none">
              <span class="text-[var(--fg-color)] font-bold">{{ stream.label }}</span>
              <span class="text-[var(--dim-color)] text-[8px]">{{ stream.pct }}% · {{ stream.resetsAt }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
