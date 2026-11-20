<script setup lang="ts">
import { computed } from 'vue';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import AppQuotaConcentric from './AppQuotaConcentric.vue';
import AppQuotaBar from './AppQuotaBar.vue';
import AppQuotaPie from './AppQuotaPie.vue';
import type { QuotaCategory, QuotaGaugeStyle } from './quotaUtils';

const props = withDefaults(
  defineProps<{
    percentage?: number;
    category: QuotaCategory;
    resetsAt?: string;
    resetEpochSeconds?: number;
    size?: number;
    width?: number;
    styleOverride?: QuotaGaugeStyle;
  }>(),
  {
    size: undefined,
    width: undefined,
    styleOverride: undefined,
  }
);

const metricsStore = useAgentMetricsStore();

const activeStyle = computed<QuotaGaugeStyle>(() => {
  return props.styleOverride || metricsStore.quotaGaugeStyle || 'concentric';
});
</script>

<template>
  <!-- Concentric Dual-Ring / Radar Clock -->
  <AppQuotaConcentric 
    v-if="activeStyle === 'concentric'"
    :category="category"
    :percentage="percentage"
    :resetsAt="resetsAt"
    :resetEpochSeconds="resetEpochSeconds"
    :size="size ?? 16"
  />

  <!-- Dual-Linear Segmented Bars -->
  <AppQuotaBar 
    v-else-if="activeStyle === 'bars'"
    :category="category"
    :percentage="percentage"
    :resetsAt="resetsAt"
    :resetEpochSeconds="resetEpochSeconds"
    :width="width ?? 22"
  />

  <!-- Classic Micro Pie Discs -->
  <AppQuotaPie 
    v-else
    :category="category"
    :percentage="percentage"
    :resetsAt="resetsAt"
    :resetEpochSeconds="resetEpochSeconds"
    :size="size ?? 13"
  />
</template>
