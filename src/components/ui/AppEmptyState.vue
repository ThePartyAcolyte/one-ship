<script setup lang="ts">
import type { Component } from 'vue';

defineProps<{
  icon: Component;
  title: string;
}>();
</script>

<template>
  <!--
    Shared empty state for workspace panels (Spec 13 §3.13): flat card, accent
    icon, uppercase title, dim description and symbol-only actions. Panels must
    use this component instead of re-declaring their own card geometry.
  -->
  <div class="flex-1 min-h-0 flex items-center justify-center p-6">
    <div class="max-w-md border border-[var(--border-color)] ui-surface-card p-6 flex flex-col items-center text-center gap-3">
      <component :is="icon" :size="32" class="text-[var(--accent-color)]" />
      <div class="text-xs font-bold uppercase tracking-wider">{{ title }}</div>
      <p v-if="$slots.description" class="text-[11px] text-[var(--dim-color)] leading-relaxed">
        <slot name="description" />
      </p>
      <div v-if="$slots.actions" class="mt-1 flex items-center gap-2">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
