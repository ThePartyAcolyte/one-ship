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
    percentage?: number; // 0 to 100 remaining percentage
    category: QuotaCategory;
    resetsAt?: string;
    resetEpochSeconds?: number;
    size?: number;
  }>(),
  {
    size: 16,
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

// Outer Clock Arc Math (Radius = 9.8, Circumference ≈ 61.575)
const outerRadius = 9.8;
const outerCircumference = 2 * Math.PI * outerRadius;

const timeStrokeOffset = computed(() => {
  const ratio = timeRatio.value;
  if (ratio === undefined || ratio === null) {
    return 0;
  }
  // Drains clockwise as time ticks down towards 0
  return outerCircumference * (1 - Math.max(0, Math.min(1, ratio)));
});

// Inner Pie Slice Math (Radius = 6.2, Center = 12, 12)
const innerRadius = 6.2;
const cx = 12;
const cy = 12;

const innerPiePath = computed(() => {
  const pct = props.percentage;
  if (pct === undefined || pct === null || pct <= 0) {
    return '';
  }
  if (pct >= 99.5) {
    return ''; // Rendered as full SVG circle for maximum rendering precision
  }

  const angleRad = (pct / 100) * 2 * Math.PI;
  const startX = cx;
  const startY = cy - innerRadius;

  const endAngle = -Math.PI / 2 + angleRad;
  const endX = cx + innerRadius * Math.cos(endAngle);
  const endY = cy + innerRadius * Math.sin(endAngle);

  const largeArcFlag = angleRad > Math.PI ? 1 : 0;

  return `M ${cx} ${cy} L ${startX.toFixed(2)} ${startY.toFixed(2)} A ${innerRadius} ${innerRadius} 0 ${largeArcFlag} 1 ${endX.toFixed(2)} ${endY.toFixed(2)} Z`;
});
</script>

<template>
  <AppTooltip :text="tooltipText" position="top">
    <div 
      class="inline-flex items-center justify-center cursor-help shrink-0 select-none"
      :style="{ width: `${size}px`, height: `${size}px` }"
    >
      <svg 
        class="overflow-visible" 
        :width="size" 
        :height="size" 
        viewBox="0 0 24 24"
      >
        <!-- ── 1. OUTER RING: Window Time Clock Track ────────────────── -->
        <circle 
          :cx="cx" 
          :cy="cy" 
          :r="outerRadius" 
          fill="none" 
          stroke="rgba(255,255,255,0.12)" 
          stroke-width="1.8" 
        />

        <!-- Untracked Time Ring (dashed) -->
        <circle 
          v-if="timeRatio === undefined" 
          :cx="cx" 
          :cy="cy" 
          :r="outerRadius" 
          fill="none" 
          :stroke="meta.hexColor" 
          stroke-width="1.8" 
          stroke-dasharray="2 2" 
          opacity="0.3" 
        />

        <!-- Active Time Remaining Clock Arc -->
        <circle 
          v-else 
          :cx="cx" 
          :cy="cy" 
          :r="outerRadius" 
          fill="none" 
          :stroke="meta.hexColor" 
          stroke-width="1.8" 
          :stroke-dasharray="outerCircumference" 
          :stroke-dashoffset="timeStrokeOffset" 
          stroke-linecap="round"
          transform="rotate(-90 12 12)" 
          opacity="0.85" 
        />

        <!-- ── 2. INNER CORE: Quota Remaining Pie ───────────────────── -->
        <!-- Background guide ring for inner pie -->
        <circle 
          :cx="cx" 
          :cy="cy" 
          :r="innerRadius" 
          fill="none" 
          stroke="rgba(255,255,255,0.06)" 
          stroke-width="1" 
        />

        <!-- Untracked Quota (dotted inner circle) -->
        <circle 
          v-if="percentage === undefined || percentage === null" 
          :cx="cx" 
          :cy="cy" 
          :r="innerRadius" 
          fill="none" 
          :stroke="meta.hexColor" 
          stroke-width="1" 
          stroke-dasharray="1.5 1.5" 
          opacity="0.35" 
        />

        <!-- Depleted Quota Indicator (Red Warning Core) -->
        <circle 
          v-else-if="percentage <= 0" 
          :cx="cx" 
          :cy="cy" 
          :r="innerRadius" 
          fill="rgba(239, 68, 68, 0.25)" 
          stroke="#ef4444" 
          stroke-width="1.4" 
        />

        <!-- 100% Quota Full Solid Disc -->
        <circle 
          v-else-if="percentage >= 99.5" 
          :cx="cx" 
          :cy="cy" 
          :r="innerRadius" 
          :fill="meta.hexColor" 
        />

        <!-- Variable Quota Pie Slice -->
        <path 
          v-else 
          :d="innerPiePath" 
          :fill="meta.hexColor" 
        />
      </svg>
    </div>
  </AppTooltip>
</template>
