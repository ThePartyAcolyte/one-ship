<script setup lang="ts">
import { ref, nextTick, onUnmounted } from 'vue';

const props = withDefaults(
  defineProps<{
    text?: string;
    position?: 'top' | 'bottom' | 'left' | 'right' | 'auto';
    disabled?: boolean;
  }>(),
  {
    position: 'auto',
    disabled: false,
  }
);

const isVisible = ref(false);
const isPositioned = ref(false);
const containerRef = ref<HTMLElement | null>(null);
const tooltipRef = ref<HTMLElement | null>(null);
const tooltipStyle = ref<Record<string, string>>({});

async function showTooltip() {
  if (props.disabled || !props.text || !containerRef.value) return;
  const triggerRect = containerRef.value.getBoundingClientRect();
  const margin = 8;
  const gap = 6;

  isPositioned.value = false;
  isVisible.value = true;
  await nextTick();

  const tipEl = tooltipRef.value;
  if (!tipEl || !isVisible.value) return;

  const tipRect = tipEl.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  let top = triggerRect.top - gap - tipRect.height;
  let left = triggerRect.left + triggerRect.width / 2 - tipRect.width / 2;

  if (props.position === 'bottom') {
    top = triggerRect.bottom + gap;
    if (top + tipRect.height > vh - margin && triggerRect.top - gap - tipRect.height >= margin) {
      top = triggerRect.top - gap - tipRect.height;
    }
  } else if (props.position === 'left') {
    top = triggerRect.top + triggerRect.height / 2 - tipRect.height / 2;
    left = triggerRect.left - gap - tipRect.width;
    if (left < margin && triggerRect.right + gap + tipRect.width <= vw - margin) {
      left = triggerRect.right + gap;
    }
  } else if (props.position === 'right') {
    top = triggerRect.top + triggerRect.height / 2 - tipRect.height / 2;
    left = triggerRect.right + gap;
    if (left + tipRect.width > vw - margin && triggerRect.left - gap - tipRect.width >= margin) {
      left = triggerRect.left - gap - tipRect.width;
    }
  } else {
    // 'top' or 'auto': prefer above, flip below if clipped at top
    if (top < margin && triggerRect.bottom + gap + tipRect.height <= vh - margin) {
      top = triggerRect.bottom + gap;
    }
  }

  // Clamp strictly inside viewport bounds
  left = Math.max(margin, Math.min(left, vw - tipRect.width - margin));
  top = Math.max(margin, Math.min(top, vh - tipRect.height - margin));

  tooltipStyle.value = {
    top: `${Math.round(top)}px`,
    left: `${Math.round(left)}px`,
  };
  isPositioned.value = true;
}

function hideTooltip() {
  isVisible.value = false;
  isPositioned.value = false;
}

onUnmounted(() => {
  isVisible.value = false;
});
</script>

<template>
  <div 
    ref="containerRef"
    class="inline-flex items-center max-w-full"
    @mouseenter="showTooltip"
    @mouseleave="hideTooltip"
    @focus="showTooltip"
    @blur="hideTooltip"
  >
    <slot />
    <Teleport to="body">
      <div
        v-if="isVisible && text && !disabled"
        ref="tooltipRef"
        class="fixed z-[99999] bg-[var(--ui-bg-title)] border border-[var(--border-color)] px-2 py-1 text-[10px] pointer-events-none text-[var(--fg-color)] font-mono shadow-2xl transition-opacity duration-100 max-w-xs md:max-w-md break-words whitespace-pre-line"
        :class="isPositioned ? 'opacity-100' : 'opacity-0'"
        :style="tooltipStyle"
      >
        {{ text }}
      </div>
    </Teleport>
  </div>
</template>
