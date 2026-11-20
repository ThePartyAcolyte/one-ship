<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';

export interface SelectOption {
  label: string;
  value: string;
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SelectOption[];
    placeholder?: string;
    disabled?: boolean;
    compact?: boolean;
  }>(),
  {
    placeholder: 'Select option...',
    disabled: false,
    compact: false,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const isOpen = ref(false);
const containerRef = ref<HTMLElement | null>(null);
const popoverRef = ref<HTMLElement | null>(null);
const highlightedIndex = ref(-1);

const popoverStyle = ref<Record<string, string>>({
  position: 'fixed',
  top: '0px',
  left: '0px',
  width: '0px',
  zIndex: '99999',
});

function updatePopoverPosition() {
  if (!containerRef.value) return;
  const rect = containerRef.value.getBoundingClientRect();
  const menuMaxHeight = 240;
  const spaceBelow = window.innerHeight - rect.bottom;
  const placeAbove = spaceBelow < menuMaxHeight && rect.top > menuMaxHeight;

  const minMenuWidth = Math.max(rect.width, 160);
  let computedLeft = rect.left;
  if (computedLeft + minMenuWidth > window.innerWidth - 8) {
    computedLeft = Math.max(8, window.innerWidth - minMenuWidth - 8);
  }

  popoverStyle.value = {
    position: 'fixed',
    top: placeAbove ? `${Math.max(4, rect.top - menuMaxHeight - 2)}px` : `${rect.bottom + 2}px`,
    left: `${computedLeft}px`,
    width: `${minMenuWidth}px`,
    zIndex: '99999',
  };
}

const selectedOption = computed(() => {
  return props.options.find(opt => opt.value === props.modelValue);
});

const displayLabel = computed(() => {
  if (selectedOption.value) {
    return selectedOption.value.label;
  }
  if (props.modelValue) {
    return props.modelValue;
  }
  return props.placeholder;
});

function toggleDropdown() {
  if (props.disabled) return;
  isOpen.value = !isOpen.value;
  if (isOpen.value) {
    const idx = props.options.findIndex(opt => opt.value === props.modelValue);
    highlightedIndex.value = idx >= 0 ? idx : 0;
    nextTick(() => {
      updatePopoverPosition();
    });
  }
}

function selectOption(opt: SelectOption) {
  if (opt.disabled) return;
  emit('update:modelValue', opt.value);
  isOpen.value = false;
}

function onKeyDown(e: KeyboardEvent) {
  if (props.disabled) return;

  if (!isOpen.value) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      toggleDropdown();
    }
    return;
  }

  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault();
      if (highlightedIndex.value < props.options.length - 1) {
        highlightedIndex.value++;
      } else {
        highlightedIndex.value = 0;
      }
      break;
    case 'ArrowUp':
      e.preventDefault();
      if (highlightedIndex.value > 0) {
        highlightedIndex.value--;
      } else {
        highlightedIndex.value = props.options.length - 1;
      }
      break;
    case 'Enter':
    case ' ':
      e.preventDefault();
      if (highlightedIndex.value >= 0 && highlightedIndex.value < props.options.length) {
        selectOption(props.options[highlightedIndex.value]);
      }
      break;
    case 'Escape':
    case 'Tab':
      isOpen.value = false;
      break;
  }
}

function handleClickOutside(event: MouseEvent) {
  const target = event.target as Node;
  if (
    containerRef.value && !containerRef.value.contains(target) &&
    (!popoverRef.value || !popoverRef.value.contains(target))
  ) {
    isOpen.value = false;
  }
}

function handleScrollOrResize() {
  if (isOpen.value) {
    updatePopoverPosition();
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside);
  window.addEventListener('scroll', handleScrollOrResize, true);
  window.addEventListener('resize', handleScrollOrResize);
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
  window.removeEventListener('scroll', handleScrollOrResize, true);
  window.removeEventListener('resize', handleScrollOrResize);
});
</script>

<template>
  <div 
    ref="containerRef"
    class="relative w-full font-mono text-xs select-none"
    @keydown="onKeyDown"
  >
    <!-- Trigger Button -->
    <button
      type="button"
      @click="toggleDropdown"
      :disabled="disabled"
      class="w-full bg-[var(--bg-color)] border text-[var(--fg-color)] focus:border-[var(--accent-color)] outline-none font-mono cursor-pointer flex items-center justify-between transition-colors"
      :class="[
        compact ? 'px-2 py-0.5 text-[10px]' : 'p-2 pr-2.5 text-xs',
        isOpen ? 'border-[var(--accent-color)] ring-1 ring-[var(--accent-color)]/30' : 'border-[var(--border-color)] hover:border-[var(--dim-color)]',
        disabled ? 'opacity-40 cursor-not-allowed' : ''
      ]"
    >
      <span class="truncate text-left flex-1 font-mono tracking-tight" :class="{ 'text-[var(--dim-color)]': !selectedOption && !modelValue }">
        {{ displayLabel }}
      </span>

      <!-- Custom Chevron Arrow Indicator -->
      <div 
        class="text-[var(--dim-color)] transition-transform duration-200 shrink-0 ml-2"
        :class="{ 'rotate-180 text-[var(--accent-color)]': isOpen }"
      >
        <svg class="w-3 h-3 fill-current" viewBox="0 0 20 20">
          <path d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" />
        </svg>
      </div>
    </button>

    <!-- Custom Brutalist Popover Menu Teleported to Body -->
    <Teleport to="body">
      <div
        v-if="isOpen"
        ref="popoverRef"
        :style="popoverStyle"
        class="bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl max-h-60 overflow-y-auto p-1 flex flex-col gap-0.5 font-mono text-xs select-none"
      >
        <div
          v-for="(opt, idx) in options"
          :key="opt.value"
          @click="selectOption(opt)"
          @mouseenter="highlightedIndex = idx"
          class="px-2.5 py-1.5 flex items-center justify-between text-xs cursor-pointer transition-colors group"
          :class="[
            opt.value === modelValue
              ? 'bg-[var(--ui-bg-card)] text-[var(--accent-color)] font-bold'
              : (highlightedIndex === idx ? 'bg-[var(--ui-bg-card)]/60 text-[var(--fg-color)]' : 'text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)]/40'),
            opt.disabled ? 'opacity-30 cursor-not-allowed' : ''
          ]"
        >
          <div class="flex items-center gap-2 truncate min-w-0">
            <span 
              class="text-[9px] shrink-0" 
              :class="opt.value === modelValue ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)] opacity-40'"
            >
              {{ opt.value === modelValue ? '●' : '○' }}
            </span>
            <span class="truncate">{{ opt.label }}</span>
          </div>

          <span 
            v-if="opt.value === modelValue" 
            class="text-[9px] text-[var(--accent-color)] uppercase tracking-wider ml-2 shrink-0 font-bold"
          >
            [✓]
          </span>
        </div>

        <div v-if="options.length === 0" class="px-3 py-2 text-[10px] text-[var(--dim-color)] text-center">
          No options available
        </div>
      </div>
    </Teleport>
  </div>
</template>
