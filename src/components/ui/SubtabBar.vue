<script setup lang="ts">
import type { Component } from 'vue';

export interface SubtabItem {
  id: string;
  label: string;
  icon?: Component;
  badge?: string | number;
  hidden?: boolean;
}

defineProps<{
  modelValue: string;
  items: SubtabItem[];
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();
</script>

<template>
  <!--
    Panel-level bar (Spec 13 §3.13): a fixed 36px row split into zones.
    The optional leading zone holds a fixed control that must never scroll away
    (e.g. the Tasks/Forge mode toggle) and is separated from the tabs by a divider.
    The tabs zone holds a small set of tabs and scrolls only when it cannot fit.
    The right zone is optional and renders caller tools (search, filters, actions);
    without a `right` slot the bar contains tabs only and the divider is omitted.
  -->
  <div class="h-9 shrink-0 border-b border-[var(--border-color)] ui-surface-title flex items-stretch font-mono">
    <div
      v-if="$slots.leading"
      class="flex items-center pl-2 pr-2 shrink-0 border-r border-[var(--border-color)]/60"
    >
      <slot name="leading" />
    </div>

    <div class="flex items-center gap-1 px-2 min-w-0 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <template v-for="item in items" :key="item.id">
        <button
          v-if="!item.hidden"
          type="button"
          @click="emit('update:modelValue', item.id)"
          class="h-6 px-2.5 border text-[9px] font-bold uppercase tracking-wider flex items-center gap-1.5 whitespace-nowrap transition-colors cursor-pointer"
          :class="modelValue === item.id
            ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
            : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-white/5'"
        >
          <component v-if="item.icon" :is="item.icon" :size="11" />
          <span>{{ item.label }}</span>
          <span
            v-if="item.badge !== undefined"
            class="text-[8px] leading-none px-1 py-0.5 border border-current"
          >
            {{ item.badge }}
          </span>
        </button>
      </template>
    </div>

    <div
      v-if="$slots.center"
      class="flex-1 min-w-0 flex items-center pl-1 pr-2"
    >
      <slot name="center" />
    </div>

    <div
      v-if="$slots.right"
      class="ml-auto flex items-center gap-2 px-2 min-w-0 overflow-x-auto border-l border-[var(--border-color)]/60 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      <slot name="right" />
    </div>
  </div>
</template>
