<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, type Component } from 'vue';

export interface ContextMenuItem {
  label: string;
  icon?: Component;
  action: () => void;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
}

const props = defineProps<{
  show: boolean;
  x: number;
  y: number;
  items: ContextMenuItem[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const menuRef = ref<HTMLElement | null>(null);
const posX = ref(props.x);
const posY = ref(props.y);

function adjustPosition() {
  const width = menuRef.value ? menuRef.value.offsetWidth : 160;
  const height = menuRef.value ? menuRef.value.offsetHeight : 120;
  
  const maxX = window.innerWidth - width - 12;
  const maxY = window.innerHeight - height - 12;

  posX.value = Math.max(12, Math.min(props.x, maxX));
  posY.value = Math.max(12, Math.min(props.y, maxY));
}

watch(() => [props.show, props.x, props.y], () => {
  if (props.show) {
    posX.value = props.x;
    posY.value = props.y;
    setTimeout(adjustPosition, 0);
  }
});

function handlePointerDown(e: PointerEvent) {
  if (!props.show) return;
  if (menuRef.value && !menuRef.value.contains(e.target as Node)) {
    emit('close');
  }
}

function handleKeyDown(e: KeyboardEvent) {
  if (!props.show) return;
  if (e.key === 'Escape') {
    emit('close');
  }
}

function onItemClick(item: ContextMenuItem) {
  if (item.disabled) return;
  emit('close');
  item.action();
}

onMounted(() => {
  window.addEventListener('pointerdown', handlePointerDown);
  window.addEventListener('keydown', handleKeyDown);
});

onUnmounted(() => {
  window.removeEventListener('pointerdown', handlePointerDown);
  window.removeEventListener('keydown', handleKeyDown);
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="show && items.length > 0"
      ref="menuRef"
      class="fixed z-[99999] bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl font-mono text-[11px] py-1 min-w-[160px] select-none text-[var(--fg-color)] animate-in fade-in zoom-in-95 duration-75"
      :style="{ top: `${posY}px`, left: `${posX}px` }"
      @contextmenu.prevent
    >
      <template v-for="(item, idx) in items" :key="item.separator ? `__sep_${idx}` : item.label">
        <div 
          v-if="item.separator" 
          class="border-b border-[var(--border-color)] my-1 opacity-60"
        />
        <div
          v-else
          class="px-2.5 py-1.5 flex items-center gap-2 transition-colors"
          :class="[
            item.disabled 
              ? 'opacity-30 cursor-not-allowed' 
              : (item.danger
                  ? 'hover:bg-[var(--status-error-color)] hover:text-white cursor-pointer text-[var(--status-error-color)]'
                  : 'hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] cursor-pointer')
          ]"
          @click.stop="onItemClick(item)"
        >
          <component v-if="item.icon" :is="item.icon" :size="13" class="shrink-0" />
          <span class="truncate flex-1">{{ item.label }}</span>
        </div>
      </template>
    </div>
  </Teleport>
</template>
