<script setup lang="ts">
defineProps<{
  modelValue: boolean;
  label?: string;
  description?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void;
}>();
</script>

<template>
  <div 
    v-if="label || description"
    class="flex items-center justify-between border border-[var(--border-color)] p-2 bg-black/20 select-none cursor-pointer transition-colors"
    :class="disabled ? 'opacity-50 cursor-not-allowed' : 'hover:border-[var(--border-color)]'"
    @click="!disabled && emit('update:modelValue', !modelValue)"
  >
    <div class="flex flex-col gap-0.5 pr-2">
      <span v-if="label" class="text-xs font-bold text-[var(--fg-color)]">{{ label }}</span>
      <span v-if="description" class="text-[10px] text-[var(--dim-color)]">
        {{ description }}
      </span>
    </div>
    <button 
      type="button"
      :disabled="disabled"
      class="w-9 h-4.5 flex items-center border transition-colors cursor-pointer p-0.5 shrink-0"
      :class="modelValue ? 'bg-[var(--accent-color)]/20 border-[var(--accent-color)] justify-end' : 'bg-black/40 border-[var(--border-color)] justify-start'"
    >
      <span 
        class="w-3 h-3"
        :class="modelValue ? 'bg-[var(--accent-color)]' : 'bg-[var(--dim-color)]'"
      />
    </button>
  </div>
  <button 
    v-else
    type="button"
    :disabled="disabled"
    @click="!disabled && emit('update:modelValue', !modelValue)"
    class="w-8 h-4 flex items-center border transition-colors cursor-pointer p-0.5 shrink-0 select-none"
    :class="[
      disabled ? 'opacity-50 cursor-not-allowed' : '',
      modelValue 
        ? 'bg-[var(--accent-color)]/20 border-[var(--accent-color)] justify-end' 
        : 'bg-black/40 border-[var(--border-color)] justify-start hover:border-[var(--dim-color)]'
    ]"
  >
    <span 
      class="w-2.5 h-2.5"
      :class="modelValue ? 'bg-[var(--accent-color)]' : 'bg-[var(--dim-color)]'"
    />
  </button>
</template>
