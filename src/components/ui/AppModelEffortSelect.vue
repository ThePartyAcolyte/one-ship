<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';

export interface ModelEffortOption {
  id: string;
  label: string;
  default_effort?: string;
  effort_levels?: string[];
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    effortValue?: string;
    models: ModelEffortOption[];
    placeholder?: string;
    disabled?: boolean;
  }>(),
  {
    effortValue: '',
    placeholder: 'Select model...',
    disabled: false,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'update:effortValue', value: string): void;
  (e: 'change', payload: { model: string; effort: string }): void;
}>();

const isOpen = ref(false);
const containerRef = ref<HTMLElement | null>(null);
const popoverRef = ref<HTMLElement | null>(null);
const flyoutRef = ref<HTMLElement | null>(null);

const activeHoveredModel = ref<ModelEffortOption | null>(null);
const highlightedIndex = ref(-1);

const popoverStyle = ref<Record<string, string>>({
  position: 'fixed',
  top: '0px',
  left: '0px',
  width: '0px',
  zIndex: '99999',
});

const flyoutStyle = ref<Record<string, string>>({
  position: 'fixed',
  top: '0px',
  left: '0px',
  zIndex: '100000',
});

function updatePopoverPosition() {
  if (!containerRef.value) return;
  const rect = containerRef.value.getBoundingClientRect();
  const menuMaxHeight = 260;
  const spaceBelow = window.innerHeight - rect.bottom;
  const placeAbove = spaceBelow < menuMaxHeight && rect.top > menuMaxHeight;

  const minMenuWidth = Math.max(rect.width, 210);
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

function updateFlyoutPosition(itemEl: HTMLElement) {
  const itemRect = itemEl.getBoundingClientRect();
  const flyoutWidth = 140;
  const flyoutHeight = 120;

  // Horizontal positioning: default right, flip to left if overflowing
  let left = itemRect.right + 2;
  if (left + flyoutWidth > window.innerWidth - 8) {
    left = Math.max(8, itemRect.left - flyoutWidth - 2);
  }

  // Vertical positioning: align top with item, adjust if bottom overflow
  let top = itemRect.top;
  if (top + flyoutHeight > window.innerHeight - 8) {
    top = Math.max(8, window.innerHeight - flyoutHeight - 8);
  }

  flyoutStyle.value = {
    position: 'fixed',
    top: `${top}px`,
    left: `${left}px`,
    width: `${flyoutWidth}px`,
    zIndex: '100000',
  };
}

const currentModel = computed(() => {
  return props.models.find(m => m.id === props.modelValue);
});

const displayModelLabel = computed(() => {
  if (currentModel.value) {
    return currentModel.value.label;
  }
  return props.modelValue || props.placeholder;
});

const currentEffort = computed(() => {
  if (!currentModel.value || !currentModel.value.effort_levels || currentModel.value.effort_levels.length === 0) {
    return '';
  }
  return props.effortValue || currentModel.value.default_effort || currentModel.value.effort_levels[0] || '';
});

function toggleDropdown() {
  if (props.disabled) return;
  isOpen.value = !isOpen.value;
  if (isOpen.value) {
    activeHoveredModel.value = null;
    const idx = props.models.findIndex(m => m.id === props.modelValue);
    highlightedIndex.value = idx >= 0 ? idx : 0;
    nextTick(() => {
      updatePopoverPosition();
    });
  }
}

function handleModelHover(model: ModelEffortOption, event: MouseEvent) {
  if (model.disabled) return;
  if (model.effort_levels && model.effort_levels.length > 0) {
    activeHoveredModel.value = model;
    nextTick(() => {
      const target = event.currentTarget as HTMLElement;
      if (target) {
        updateFlyoutPosition(target);
      }
    });
  } else {
    activeHoveredModel.value = null;
  }
}

function selectModelWithEffort(model: ModelEffortOption, effort: string) {
  if (model.disabled) return;
  emit('update:modelValue', model.id);
  emit('update:effortValue', effort);
  emit('change', { model: model.id, effort });
  isOpen.value = false;
  activeHoveredModel.value = null;
}

function handleModelClick(model: ModelEffortOption, event: MouseEvent) {
  if (model.disabled) return;
  if (model.effort_levels && model.effort_levels.length > 0) {
    // If clicking directly on a model with effort, toggle flyout or pick default if not picking
    if (activeHoveredModel.value?.id === model.id) {
      // Pick current or default effort
      const effort = (props.effortValue && model.effort_levels.includes(props.effortValue))
        ? props.effortValue
        : (model.default_effort || model.effort_levels[0] || '');
      selectModelWithEffort(model, effort);
    } else {
      activeHoveredModel.value = model;
      nextTick(() => {
        const target = event.currentTarget as HTMLElement;
        if (target) {
          updateFlyoutPosition(target);
        }
      });
    }
  } else {
    selectModelWithEffort(model, '');
  }
}

function handleClickOutside(event: MouseEvent) {
  const target = event.target as Node;
  if (
    containerRef.value && !containerRef.value.contains(target) &&
    (!popoverRef.value || !popoverRef.value.contains(target)) &&
    (!flyoutRef.value || !flyoutRef.value.contains(target))
  ) {
    isOpen.value = false;
    activeHoveredModel.value = null;
  }
}

function handleScrollOrResize() {
  if (isOpen.value) {
    updatePopoverPosition();
    activeHoveredModel.value = null;
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
  >
    <!-- Trigger Button -->
    <button
      type="button"
      @click="toggleDropdown"
      :disabled="disabled"
      class="w-full bg-[var(--bg-color)] border p-2 text-xs text-[var(--fg-color)] focus:border-[var(--accent-color)] outline-none font-mono cursor-pointer flex items-center justify-between transition-colors pr-2.5"
      :class="[
        isOpen ? 'border-[var(--accent-color)] ring-1 ring-[var(--accent-color)]/30' : 'border-[var(--border-color)] hover:border-[var(--dim-color)]',
        disabled ? 'opacity-40 cursor-not-allowed' : ''
      ]"
    >
      <div class="truncate text-left flex-1 font-mono tracking-tight flex items-center gap-1.5 min-w-0">
        <span class="truncate" :class="{ 'text-[var(--dim-color)]': !currentModel && !modelValue }">
          {{ displayModelLabel }}
        </span>
        <span 
          v-if="currentEffort"
          class="shrink-0 px-1 py-0.2 text-[9px] font-bold uppercase tracking-wider border border-[var(--accent-color)]/40 text-[var(--accent-color)] bg-[var(--accent-color)]/10 rounded-none"
        >
          {{ currentEffort }}
        </span>
      </div>

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

    <!-- Main Popover Menu Teleported to Body -->
    <Teleport to="body">
      <div
        v-if="isOpen"
        ref="popoverRef"
        :style="popoverStyle"
        class="bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl max-h-64 overflow-y-auto p-1 flex flex-col gap-0.5 font-mono text-xs select-none"
      >
        <div
          v-for="(m, idx) in models"
          :key="m.id"
          @click="handleModelClick(m, $event)"
          @mouseenter="handleModelHover(m, $event); highlightedIndex = idx"
          class="px-2.5 py-1.5 flex items-center justify-between text-xs cursor-pointer transition-colors group relative"
          :class="[
            m.id === modelValue
              ? 'bg-[var(--ui-bg-card)] text-[var(--accent-color)] font-bold'
              : (highlightedIndex === idx ? 'bg-[var(--ui-bg-card)]/60 text-[var(--fg-color)]' : 'text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)]/40'),
            m.disabled ? 'opacity-30 cursor-not-allowed' : ''
          ]"
        >
          <div class="flex items-center gap-2 truncate min-w-0 flex-1">
            <span 
              class="text-[9px] shrink-0" 
              :class="m.id === modelValue ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)] opacity-40'"
            >
              {{ m.id === modelValue ? '●' : '○' }}
            </span>
            <span class="truncate">{{ m.label }}</span>
          </div>

          <!-- Right side indicators -->
          <div class="flex items-center gap-1.5 ml-2 shrink-0">
            <!-- Effort indicator badge if model is currently active -->
            <span
              v-if="m.id === modelValue && currentEffort"
              class="text-[8px] uppercase tracking-wider px-1 py-0.2 border border-[var(--accent-color)]/60 bg-[var(--accent-color)]/10 text-[var(--accent-color)] font-bold"
            >
              {{ currentEffort }}
            </span>

            <!-- Submenu indicator chevron if model supports multiple effort levels -->
            <span 
              v-if="m.effort_levels && m.effort_levels.length > 0"
              class="text-[9px] text-[var(--dim-color)] group-hover:text-[var(--accent-color)] transition-colors pl-1"
            >
              ▶
            </span>
            <span 
              v-else-if="m.id === modelValue" 
              class="text-[9px] text-[var(--accent-color)] uppercase tracking-wider font-bold"
            >
              [✓]
            </span>
          </div>
        </div>

        <div v-if="models.length === 0" class="px-3 py-2 text-[10px] text-[var(--dim-color)] text-center">
          No models available
        </div>
      </div>
    </Teleport>

    <!-- Submenu Flyout for Reasoning Effort Teleported to Body -->
    <Teleport to="body">
      <div
        v-if="isOpen && activeHoveredModel && activeHoveredModel.effort_levels && activeHoveredModel.effort_levels.length > 0"
        ref="flyoutRef"
        :style="flyoutStyle"
        class="bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl p-1 flex flex-col gap-0.5 font-mono text-xs select-none"
        @mouseleave="activeHoveredModel = null"
      >
        <div class="px-2 py-1 text-[9px] text-[var(--dim-color)] border-b border-[var(--border-color)]/50 uppercase tracking-widest font-bold mb-0.5 flex items-center justify-between">
          <span>Effort</span>
          <span class="text-[8px] opacity-60">{{ activeHoveredModel.label }}</span>
        </div>

        <div
          v-for="lvl in activeHoveredModel.effort_levels"
          :key="lvl"
          @click.stop="selectModelWithEffort(activeHoveredModel!, lvl)"
          class="px-2.5 py-1.5 flex items-center justify-between text-xs cursor-pointer transition-colors"
          :class="[
            (activeHoveredModel.id === modelValue && currentEffort === lvl)
              ? 'bg-[var(--ui-bg-card)] text-[var(--accent-color)] font-bold'
              : 'text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)]/60'
          ]"
        >
          <div class="flex items-center gap-2">
            <span 
              class="text-[9px]"
              :class="(activeHoveredModel.id === modelValue && currentEffort === lvl) ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)] opacity-40'"
            >
              {{ (activeHoveredModel.id === modelValue && currentEffort === lvl) ? '●' : '○' }}
            </span>
            <span class="capitalize text-xs font-mono">{{ lvl }}</span>
          </div>

          <span 
            v-if="lvl === activeHoveredModel.default_effort"
            class="text-[8px] text-[var(--dim-color)] opacity-70 ml-1.5 uppercase tracking-wider"
          >
            def
          </span>
          <span 
            v-if="activeHoveredModel.id === modelValue && currentEffort === lvl"
            class="text-[9px] text-[var(--accent-color)] font-bold ml-1.5"
          >
            [✓]
          </span>
        </div>
      </div>
    </Teleport>
  </div>
</template>
