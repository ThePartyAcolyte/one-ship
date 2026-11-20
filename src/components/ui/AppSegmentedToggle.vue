<script setup lang="ts">
import { computed, type Component } from 'vue';

export interface SegmentedItem {
  id: string;
  label: string;
  icon?: Component;
  hidden?: boolean;
}

const props = defineProps<{
  modelValue: string;
  items: SegmentedItem[];
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const visibleItems = computed(() => props.items.filter((item) => !item.hidden));
</script>

<template>
  <!--
    Strict two-or-more option switch (Spec 13 §3.13): a fixed control that
    replaces a full tab row when the selection is a mode rather than a view.
    Inactive options stay quiet; the active one carries the accent fill.
  -->
  <div class="shrink-0 h-6 flex items-stretch border border-[var(--border-color)] select-none">
    <button
      v-for="(item, index) in visibleItems"
      :key="item.id"
      type="button"
      :aria-pressed="modelValue === item.id"
      @click="emit('update:modelValue', item.id)"
      class="px-2.5 text-[9px] font-bold uppercase tracking-wider flex items-center gap-1.5 whitespace-nowrap transition-colors cursor-pointer"
      :class="[
        index > 0 ? 'border-l border-[var(--border-color)]' : '',
        modelValue === item.id
          ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
          : 'text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-white/5',
      ]"
    >
      <component v-if="item.icon" :is="item.icon" :size="11" />
      <span>{{ item.label }}</span>
    </button>
  </div>
</template>
