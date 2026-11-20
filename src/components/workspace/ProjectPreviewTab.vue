<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { Monitor, Tablet, Smartphone, Maximize2, Minimize2, Globe, ExternalLink } from '@lucide/vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import { AppTooltip } from '../ui';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
}>();

const emit = defineEmits<{
  (e: 'fullscreen-change', value: boolean): void;
}>();

type ViewportMode = 'desktop' | 'tablet' | 'mobile';

const viewportMode = ref<ViewportMode>('desktop');
const isFullscreen = ref(false);
const iframeKey = ref(0);

const previewUrl = computed(() => props.project?.preview_url ?? '');

const viewportWidthClass = computed(() => {
  if (viewportMode.value === 'mobile') return 'w-[390px] max-w-full';
  if (viewportMode.value === 'tablet') return 'w-[768px] max-w-full';
  return 'w-full';
});

function reloadPreview() {
  iframeKey.value += 1;
}

function toggleFullscreen() {
  isFullscreen.value = !isFullscreen.value;
}

// Fullscreen is a whole-window mode: the workspace hides the project rail and
// the agent bar, so the preview can own the viewport without stacking fights.
watch(isFullscreen, (value) => {
  emit('fullscreen-change', value);
});

async function openExternalBrowser() {
  if (!previewUrl.value) return;
  try {
    await openUrl(previewUrl.value);
  } catch (err: unknown) {
    console.error('Failed to open external browser:', err);
  }
}

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isFullscreen.value) {
    isFullscreen.value = false;
  }
}

function handleContextRefresh(e: Event) {
  const tab = (e as CustomEvent<{ tab?: string }>).detail?.tab;
  if (tab === 'preview') {
    reloadPreview();
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeyDown);
  window.addEventListener('oneship:context-refresh', handleContextRefresh);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown);
  window.removeEventListener('oneship:context-refresh', handleContextRefresh);
  if (isFullscreen.value) {
    isFullscreen.value = false;
    emit('fullscreen-change', false);
  }
});
</script>

<template>
  <div
    class="flex flex-col bg-[var(--bg-color)] text-[var(--fg-color)] font-mono overflow-hidden"
    :class="isFullscreen ? 'fixed inset-0 z-[45]' : 'h-full'"
  >
    <!-- Minimalist Browser Bar -->
    <div class="h-9 px-3 ui-surface-title border-b border-[var(--border-color)] flex items-center justify-between gap-3 shrink-0">
      <div class="flex items-center gap-2 min-w-0 flex-1">
        <Globe class="w-3.5 h-3.5 text-[var(--accent-color)] shrink-0" />
        <span class="text-[10px] text-[var(--fg-color)] truncate max-w-md">
          {{ previewUrl }}
        </span>
      </div>

      <!-- Responsive Presets -->
      <div class="flex items-center h-6 border border-[var(--border-color)] shrink-0">
        <AppTooltip text="Desktop (100%)">
          <button
            type="button"
            class="h-6 w-6 flex items-center justify-center cursor-pointer transition-colors"
            :class="viewportMode === 'desktop' ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)]' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="viewportMode = 'desktop'"
          >
            <Monitor class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>
        <AppTooltip text="Tablet (768px)">
          <button
            type="button"
            class="h-6 w-6 flex items-center justify-center border-x border-[var(--border-color)] cursor-pointer transition-colors"
            :class="viewportMode === 'tablet' ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)]' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="viewportMode = 'tablet'"
          >
            <Tablet class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>
        <AppTooltip text="Mobile (390px)">
          <button
            type="button"
            class="h-6 w-6 flex items-center justify-center cursor-pointer transition-colors"
            :class="viewportMode === 'mobile' ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)]' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="viewportMode = 'mobile'"
          >
            <Smartphone class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>
      </div>

      <!-- Fullscreen & External Browser -->
      <div class="flex items-center gap-1.5 shrink-0">
        <AppTooltip text="Open in external browser">
          <button
            type="button"
            class="h-6 px-2 text-[9px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer transition-colors"
            @click="openExternalBrowser"
          >
            <ExternalLink class="w-3 h-3" />
            <span>Browser</span>
          </button>
        </AppTooltip>
        <AppTooltip :text="isFullscreen ? 'Exit fullscreen (Esc)' : 'Fullscreen preview'">
          <button
            type="button"
            class="h-6 px-2 text-[9px] uppercase border flex items-center gap-1 cursor-pointer transition-colors"
            :class="isFullscreen
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
            @click="toggleFullscreen"
          >
            <Minimize2 v-if="isFullscreen" class="w-3 h-3" />
            <Maximize2 v-else class="w-3 h-3" />
            <span>{{ isFullscreen ? 'Exit (Esc)' : 'Fullscreen' }}</span>
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Live Iframe Canvas -->
    <div class="flex-1 min-h-0 flex justify-center bg-[var(--bg-color)] p-3 overflow-auto">
      <div
        class="h-full border border-[var(--border-color)] bg-white shadow-2xl transition-all duration-200 overflow-hidden"
        :class="viewportWidthClass"
      >
        <iframe
          v-if="previewUrl"
          :key="iframeKey"
          :src="previewUrl"
          class="w-full h-full border-none"
          allow="accelerometer; camera; encrypted-media; geolocation; gyroscope; microphone; midi"
        />
      </div>
    </div>
  </div>
</template>
