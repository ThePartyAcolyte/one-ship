<script setup lang="ts">
import { computed } from 'vue';
import AppTooltip from './AppTooltip.vue';
import {
  type QuotaCategory,
  getCategoryMeta,
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
    size: 13,
  }
);

const meta = computed(() => getCategoryMeta(props.category));

const tooltipText = computed(() =>
  formatQuotaTooltip(meta.value.label, props.percentage, props.resetsAt, props.resetEpochSeconds)
);

// Calculate SVG pie slice path
const svgPiePath = computed(() => {
  const pct = props.percentage;
  if (pct === undefined || pct === null || pct <= 0) {
    return '';
  }
  if (pct >= 99.9) {
    return 'M 6 1 A 5 5 0 1 1 5.999 1 Z'; // Full circle
  }

  const cx = 6;
  const cy = 6;
  const r = 5;
  const angleRad = (pct / 100) * 2 * Math.PI;

  // Start at top (-90 deg = -pi/2 rad)
  const x1 = cx;
  const y1 = cy - r; // (6, 1)

  // End point sweeping clockwise
  const endAngle = -Math.PI / 2 + angleRad;
  const x2 = cx + r * Math.cos(endAngle);
  const y2 = cy + r * Math.sin(endAngle);

  const largeArcFlag = angleRad > Math.PI ? 1 : 0;

  return `M ${cx} ${cy} L ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 ${largeArcFlag} 1 ${x2.toFixed(2)} ${y2.toFixed(2)} Z`;
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
        viewBox="0 0 12 12"
      >
        <!-- Background outer ring -->
        <circle 
          cx="6" 
          cy="6" 
          r="5" 
          fill="none" 
          stroke="rgba(255,255,255,0.15)" 
          stroke-width="1" 
        />

        <!-- Untracked indicator (dotted ring) -->
        <circle 
          v-if="percentage === undefined || percentage === null" 
          cx="6" 
          cy="6" 
          r="5" 
          fill="none" 
          :stroke="meta.hexColor" 
          stroke-width="1" 
          stroke-dasharray="1.5 1.5" 
          opacity="0.4" 
        />

        <!-- Depleted quota indicator (red warning circle) -->
        <circle 
          v-else-if="percentage <= 0" 
          cx="6" 
          cy="6" 
          r="5" 
          fill="rgba(239, 68, 68, 0.2)" 
          stroke="#ef4444" 
          stroke-width="1.5" 
        />

        <!-- Filled pie slice arc -->
        <path 
          v-else 
          :d="svgPiePath" 
          :fill="meta.hexColor" 
        />
      </svg>
    </div>
  </AppTooltip>
</template>
