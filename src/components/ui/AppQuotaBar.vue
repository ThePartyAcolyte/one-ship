<script setup lang="ts">
import { computed } from 'vue';
import AppTooltip from './AppTooltip.vue';
import {
  type QuotaCategory,
  getCategoryMeta,
  calculateTimeRemainingRatio,
  formatQuotaTooltip,
} from './quotaUtils';

const props = withDefaults(
  defineProps<{
    percentage?: number;
    category: QuotaCategory;
    resetsAt?: string;
    resetEpochSeconds?: number;
    width?: number;
  }>(),
  {
    width: 22,
  }
);

const meta = computed(() => getCategoryMeta(props.category));

const tooltipText = computed(() =>
  formatQuotaTooltip(meta.value.label, props.percentage, props.resetsAt, props.resetEpochSeconds)
);

const timeRatio = computed(() => {
  return calculateTimeRemainingRatio(
    props.resetsAt,
    props.resetEpochSeconds,
    meta.value.windowSeconds
  );
});

const quotaClampedPct = computed(() => {
  if (props.percentage === undefined || props.percentage === null) return 0;
  return Math.max(0, Math.min(100, props.percentage));
});

const timeClampedPct = computed(() => {
  if (timeRatio.value === undefined || timeRatio.value === null) return 0;
  return Math.max(0, Math.min(100, timeRatio.value * 100));
});
</script>

<template>
  <AppTooltip :text="tooltipText" position="top">
    <div 
      class="inline-flex flex-col justify-center gap-0.5 cursor-help shrink-0 select-none py-0.5"
      :style="{ width: `${width}px` }"
    >
      <!-- ── TOP BAR: Quota Capacity Level ────────────────────── -->
      <div class="w-full h-1.5 bg-white/10 rounded-[0.5px] overflow-hidden relative">
        <!-- Untracked placeholder -->
        <div 
          v-if="percentage === undefined || percentage === null" 
          class="w-full h-full border-t border-dashed opacity-30"
          :style="{ borderColor: meta.hexColor }"
        />
        <!-- Depleted warning -->
        <div 
          v-else-if="percentage <= 0" 
          class="w-full h-full bg-[var(--status-error-color)]/80 animate-pulse"
        />
        <!-- Active capacity fill -->
        <div 
          v-else 
          class="h-full transition-all duration-300 rounded-[0.5px]"
          :style="{ 
            width: `${quotaClampedPct}%`, 
            backgroundColor: meta.hexColor 
          }"
        />
      </div>

      <!-- ── BOTTOM BAR: Time Window Decay Gauge ──────────────── -->
      <div class="w-full h-[2px] bg-white/5 rounded-[0.5px] overflow-hidden relative">
        <!-- Untracked time -->
        <div 
          v-if="timeRatio === undefined" 
          class="w-full h-full opacity-20"
          :style="{ backgroundColor: meta.hexColor }"
        />
        <!-- Active time remaining bar -->
        <div 
          v-else 
          class="h-full transition-all duration-300 rounded-[0.5px]"
          :style="{ 
            width: `${timeClampedPct}%`, 
            backgroundColor: meta.hexColor,
            opacity: 0.75
          }"
        />
      </div>
    </div>
  </AppTooltip>
</template>
