<script setup lang="ts">
import { ref } from 'vue';
import { ChevronUp, ChevronDown } from '@lucide/vue';

const inputEl = ref<HTMLInputElement | null>(null);
defineExpose({
  focus: () => inputEl.value?.focus(),
});

const props = withDefaults(
  defineProps<{
    modelValue: string | number;
    type?: string;
    placeholder?: string;
    disabled?: boolean;
    min?: number;
    max?: number;
    step?: number;
  }>(),
  {
    type: 'text',
    placeholder: '',
    disabled: false,
    step: 1,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string | number): void;
}>();

function onInput(event: Event) {
  const target = event.target as HTMLInputElement;
  if (props.type === 'number') {
    const num = target.value === '' ? '' : Number(target.value);
    emit('update:modelValue', num);
  } else {
    emit('update:modelValue', target.value);
  }
}

function parseCurrentValue(): number {
  const val = typeof props.modelValue === 'number' ? props.modelValue : parseFloat(String(props.modelValue));
  return isNaN(val) ? 0 : val;
}

function roundToPrecision(val: number, stepVal: number): number {
  const stepStr = stepVal.toString();
  const decimalIdx = stepStr.indexOf('.');
  const precision = decimalIdx >= 0 ? stepStr.length - decimalIdx - 1 : 0;
  return precision > 0 ? parseFloat(val.toFixed(precision)) : Math.round(val);
}

function increment() {
  if (props.disabled) return;
  const current = parseCurrentValue();
  const stepVal = props.step ?? 1;
  let next = current + stepVal;
  if (props.max !== undefined && next > props.max) {
    next = props.max;
  }
  const result = roundToPrecision(next, stepVal);
  emit('update:modelValue', typeof props.modelValue === 'number' ? result : String(result));
}

function decrement() {
  if (props.disabled) return;
  const current = parseCurrentValue();
  const stepVal = props.step ?? 1;
  let next = current - stepVal;
  if (props.min !== undefined && next < props.min) {
    next = props.min;
  }
  const result = roundToPrecision(next, stepVal);
  emit('update:modelValue', typeof props.modelValue === 'number' ? result : String(result));
}
</script>

<template>
  <!-- Number Input with Stacked Brutalist Stepper Square -->
  <div
    v-if="type === 'number'"
    class="relative flex items-stretch w-full bg-[var(--bg-color)]/60 border border-[var(--border-color)] focus-within:border-[var(--accent-color)] transition-colors h-[32px] select-none"
    :class="disabled ? 'opacity-50 cursor-not-allowed' : ''"
  >
    <input
      type="number"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :min="min"
      :max="max"
      :step="step"
      @input="onInput"
      class="w-full bg-transparent px-2.5 py-1 text-xs text-[var(--fg-color)] outline-none font-mono placeholder:text-[var(--dim-color)]/50 transition-colors disabled:cursor-not-allowed [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
    />
    
    <!-- Square Stepper Container on Right -->
    <div class="h-full aspect-square border-l border-[var(--border-color)] flex flex-col shrink-0">
      <!-- Top Increment Button -->
      <button
        type="button"
        :disabled="disabled || (max !== undefined && parseCurrentValue() >= max)"
        @click="increment"
        class="h-1/2 w-full flex items-center justify-center border-b border-[var(--border-color)] text-[var(--dim-color)] hover:text-black hover:bg-[var(--accent-color)] active:opacity-70 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <ChevronUp :size="10" />
      </button>
      
      <!-- Bottom Decrement Button -->
      <button
        type="button"
        :disabled="disabled || (min !== undefined && parseCurrentValue() <= min)"
        @click="decrement"
        class="h-1/2 w-full flex items-center justify-center text-[var(--dim-color)] hover:text-black hover:bg-[var(--accent-color)] active:opacity-70 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <ChevronDown :size="10" />
      </button>
    </div>
  </div>

  <!-- Standard Text / Other Input -->
  <input
    v-else
    ref="inputEl"
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    @input="onInput"
    class="w-full bg-[var(--bg-color)]/60 border border-[var(--border-color)] px-2.5 py-1.5 text-xs text-[var(--fg-color)] focus:border-[var(--accent-color)] outline-none font-mono placeholder:text-[var(--dim-color)]/50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed h-[32px]"
  />
</template>
