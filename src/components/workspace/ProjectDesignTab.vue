<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, computed, nextTick } from 'vue';
import {
  ExternalLink,
  Loader2,
  Layers,
  PanelLeft,
  ZoomIn,
  ZoomOut,
  Maximize2,
} from '@lucide/vue';
import { AppTooltip, AppSelect, AppEmptyState } from '../ui';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import type { ProjectRecord } from '../../types';
import type { DesigmaStatus } from '../../types/generated/DesigmaStatus';
import type { DesigmaPreviewResult } from '../../types/generated/DesigmaPreviewResult';

type FitMode = 'fit-width' | 'fit-all' | 'free';

const props = defineProps<{
  project: ProjectRecord;
}>();

const status = ref<DesigmaStatus | null>(null);
const selectedFile = ref<string>('');
const selectedNodeId = ref<number | null>(null);
const preview = ref<DesigmaPreviewResult | null>(null);
const isLoadingPreview = ref(false);
const isLaunching = ref(false);
const showLayers = ref(true);
const errorMessage = ref<string | null>(null);

// Navigation, zoom, and reactive fit-mode state
const viewportRef = ref<HTMLElement | null>(null);
const svgContainerRef = ref<HTMLElement | null>(null);
const fitMode = ref<FitMode>('fit-width');
const zoom = ref(1);
const svgWidth = ref(0);
const svgHeight = ref(0);
const isPanning = ref(false);
let panStartX = 0;
let panStartY = 0;
let panScrollLeft = 0;
let panScrollTop = 0;

let unlistenFileChanged: (() => void) | null = null;
let resizeObserver: ResizeObserver | null = null;

const fileOptions = computed(() => {
  const files = status.value?.design_files ?? [];
  return files.map((f) => ({ value: f, label: f }));
});

const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`);

const fitModeLabel = computed(() => {
  if (fitMode.value === 'fit-width') return 'Fit Width';
  if (fitMode.value === 'fit-all') return 'Fit All';
  return 'Free';
});

const scaledStageStyle = computed(() => {
  if (!svgWidth.value || !svgHeight.value) return {};
  return {
    width: `${Math.max(1, Math.round(svgWidth.value * zoom.value))}px`,
    height: `${Math.max(1, Math.round(svgHeight.value * zoom.value))}px`,
  };
});

const svgScaleStyle = computed(() => {
  if (!svgWidth.value || !svgHeight.value) return {};
  return {
    width: `${svgWidth.value}px`,
    height: `${svgHeight.value}px`,
    transform: `scale(${zoom.value})`,
    transformOrigin: 'top left',
  };
});

function measureSvgDimensions() {
  const svgEl = svgContainerRef.value?.querySelector('svg');
  if (!svgEl) {
    svgWidth.value = 0;
    svgHeight.value = 0;
    return;
  }
  let w = Number(svgEl.getAttribute('width'));
  let h = Number(svgEl.getAttribute('height'));
  if ((!w || !h) && svgEl.viewBox?.baseVal) {
    w = svgEl.viewBox.baseVal.width;
    h = svgEl.viewBox.baseVal.height;
  }
  svgWidth.value = w > 0 ? w : 1280;
  svgHeight.value = h > 0 ? h : 800;
}

function applyFitMode() {
  if (fitMode.value === 'free') return;
  const vp = viewportRef.value;
  if (!vp || !svgWidth.value || !svgHeight.value) return;

  const availW = Math.max(100, vp.clientWidth - 48);
  const availH = Math.max(100, vp.clientHeight - 48);
  const nextScale =
    fitMode.value === 'fit-width'
      ? availW / svgWidth.value
      : Math.min(availW / svgWidth.value, availH / svgHeight.value);

  zoom.value = Math.min(4, Math.max(0.05, Number(nextScale.toFixed(3))));
}

function cycleFitMode() {
  if (fitMode.value === 'fit-width') {
    fitMode.value = 'fit-all';
  } else if (fitMode.value === 'fit-all') {
    fitMode.value = 'free';
  } else {
    fitMode.value = 'fit-width';
  }
  applyFitMode();
}

function resetTo100() {
  fitMode.value = 'free';
  zoom.value = 1;
}

function adjustZoom(delta: number) {
  fitMode.value = 'free';
  zoom.value = Math.min(8, Math.max(0.05, Number((zoom.value + delta).toFixed(2))));
}

function handleWheel(ev: WheelEvent) {
  const vp = viewportRef.value;
  if (!vp) return;

  if (ev.ctrlKey || ev.metaKey) {
    ev.preventDefault();
    fitMode.value = 'free';
    const factor = ev.deltaY < 0 ? 1.12 : 0.89;
    zoom.value = Number(Math.min(8, Math.max(0.05, zoom.value * factor)).toFixed(3));
    return;
  }

  if (ev.altKey) {
    ev.preventDefault();
    vp.scrollLeft += ev.deltaY !== 0 ? ev.deltaY : ev.deltaX;
  }
}

function handlePointerDown(ev: PointerEvent) {
  if (fitMode.value !== 'free' || ev.button !== 0) return;
  const vp = viewportRef.value;
  if (!vp) return;
  isPanning.value = true;
  panStartX = ev.clientX;
  panStartY = ev.clientY;
  panScrollLeft = vp.scrollLeft;
  panScrollTop = vp.scrollTop;
  (ev.currentTarget as HTMLElement | null)?.setPointerCapture?.(ev.pointerId);
}

function handlePointerMove(ev: PointerEvent) {
  if (!isPanning.value) return;
  const vp = viewportRef.value;
  if (!vp) return;
  vp.scrollLeft = panScrollLeft - (ev.clientX - panStartX);
  vp.scrollTop = panScrollTop - (ev.clientY - panStartY);
}

function handlePointerUp(ev: PointerEvent) {
  if (!isPanning.value) return;
  isPanning.value = false;
  (ev.currentTarget as HTMLElement | null)?.releasePointerCapture?.(ev.pointerId);
}

function setupViewportObserver() {
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  if (!viewportRef.value) return;
  resizeObserver = new ResizeObserver(() => {
    applyFitMode();
  });
  resizeObserver.observe(viewportRef.value);
}

async function loadStatusAndPreview(forceRender = true, resetScroll = false) {
  if (!props.project?.path) return;
  try {
    const nextStatus = await daemonCall('get_project_desigma_status', {
      project_path: props.project.path,
    });
    status.value = nextStatus;

    if (nextStatus.design_files.length === 0) {
      selectedFile.value = '';
      selectedNodeId.value = null;
      preview.value = null;
      return;
    }

    if (!selectedFile.value || !nextStatus.design_files.includes(selectedFile.value)) {
      selectedFile.value = nextStatus.design_files[0];
      selectedNodeId.value = null;
    }

    if (forceRender) {
      await refreshPreview(undefined, resetScroll);
    }
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  }
}

async function refreshPreview(nodeIdOverride?: number | null, resetScroll = false) {
  if (!props.project?.path || !selectedFile.value || isLoadingPreview.value) return;
  const targetNode = nodeIdOverride !== undefined ? nodeIdOverride : selectedNodeId.value;
  isLoadingPreview.value = true;
  errorMessage.value = null;
  try {
    const res = await daemonCall('render_desigma_preview', {
      project_path: props.project.path,
      file_name: selectedFile.value,
      node_id: targetNode,
    });
    preview.value = res;
    selectedNodeId.value = res.selected_node_id;
    await nextTick();
    setupViewportObserver();
    measureSvgDimensions();
    applyFitMode();
    if (resetScroll && viewportRef.value) {
      viewportRef.value.scrollTop = 0;
      viewportRef.value.scrollLeft = 0;
    }
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLoadingPreview.value = false;
  }
}

function selectNode(nodeId: number) {
  selectedNodeId.value = nodeId;
  void refreshPreview(nodeId, true);
}

async function handleOpenEditor() {
  if (!props.project?.path || isLaunching.value) return;
  isLaunching.value = true;
  errorMessage.value = null;
  try {
    status.value = await daemonCall('launch_project_desigma', {
      project_path: props.project.path,
    });
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLaunching.value = false;
  }
}

watch(() => props.project?.path, () => {
  selectedFile.value = '';
  selectedNodeId.value = null;
  preview.value = null;
  void loadStatusAndPreview(true, true);
});

watch(selectedFile, (next, prev) => {
  if (next && next !== prev) {
    selectedNodeId.value = null;
    void refreshPreview(null, true);
  }
});

onMounted(() => {
  void loadStatusAndPreview(true, true);
  unlistenFileChanged = onDaemonEvent('desigma-file-changed', (payload) => {
    if (payload.project_path !== props.project?.path) return;
    if (!selectedFile.value || payload.file_name === selectedFile.value) {
      void loadStatusAndPreview(true, false);
    }
  });
});

onUnmounted(() => {
  if (unlistenFileChanged) {
    unlistenFileChanged();
    unlistenFileChanged = null;
  }
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--bg-color)] text-[var(--fg-color)] font-mono overflow-hidden">
    <!-- Top Toolbar -->
    <div class="h-9 px-3 ui-surface-title border-b border-[var(--border-color)] flex items-center justify-between gap-3 shrink-0">
      <div class="flex items-center gap-2 min-w-0 overflow-x-auto scrollbar-none">
        <AppTooltip text="Toggle pages and layers sidebar">
          <button
            type="button"
            class="ui-icon-btn"
            :class="{ '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15': showLayers }"
            @click="showLayers = !showLayers"
          >
            <PanelLeft class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <div v-if="fileOptions.length > 0" class="w-44 shrink-0">
          <AppSelect
            v-model="selectedFile"
            :options="fileOptions"
            placeholder="Select design file..."
            compact
            class="[&>button]:h-6"
          />
        </div>
        <span v-else class="text-[10px] text-[var(--dim-color)] whitespace-nowrap">
          No design files in {{ status?.design_dir_path ?? 'design/' }}
        </span>
      </div>

      <div class="flex items-center gap-1.5 shrink-0">
        <!-- Zoom & Fit Controls -->
        <div class="flex items-center h-6 border border-[var(--border-color)]">
          <AppTooltip text="Zoom out">
            <button
              type="button"
              class="h-6 w-6 flex items-center justify-center text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
              @click="adjustZoom(-0.2)"
            >
              <ZoomOut class="w-3 h-3" />
            </button>
          </AppTooltip>
          <span class="h-6 px-2 flex items-center text-[9px] tabular-nums border-x border-[var(--border-color)]">
            {{ zoomPercent }}
          </span>
          <AppTooltip text="Zoom in">
            <button
              type="button"
              class="h-6 w-6 flex items-center justify-center text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
              @click="adjustZoom(0.2)"
            >
              <ZoomIn class="w-3 h-3" />
            </button>
          </AppTooltip>
          <AppTooltip text="Cycle view mode: Fit Width / Fit All / Free">
            <button
              type="button"
              class="h-6 px-2 text-[9px] uppercase border-l border-[var(--border-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer transition-colors"
              :class="{ 'text-[var(--accent-color)] font-bold': fitMode !== 'free' }"
              @click="cycleFitMode"
            >
              <Maximize2 class="w-3 h-3" />
              <span>{{ fitModeLabel }}</span>
            </button>
          </AppTooltip>
          <AppTooltip text="Reset to 100% scale">
            <button
              type="button"
              class="h-6 px-2 text-[9px] uppercase border-l border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer transition-colors"
              @click="resetTo100"
            >
              100%
            </button>
          </AppTooltip>
        </div>

        <AppTooltip :text="status?.is_editor_running ? 'Focus One-Ship Designer' : 'Open One-Ship Designer'">
          <button
            type="button"
            class="ui-icon-btn disabled:opacity-50"
            :disabled="isLaunching"
            @click="handleOpenEditor"
          >
            <Loader2 v-if="isLaunching" class="w-3.5 h-3.5 animate-spin" />
            <ExternalLink v-else class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Error Banner -->
    <div
      v-if="errorMessage"
      class="px-4 py-1.5 bg-[var(--status-error-bg)] border-b border-[var(--status-error-border)] text-[var(--status-error-color)] text-[11px] flex items-center justify-between"
    >
      <span>{{ errorMessage }}</span>
      <button type="button" class="underline text-[10px] cursor-pointer" @click="errorMessage = null">
        Dismiss
      </button>
    </div>

    <!-- Main Split: Sidebar (Pages + Layers) & Scrollable Vector Viewport -->
    <div class="flex-1 min-h-0 flex overflow-hidden">
      <!-- Pages & Layers Sidebar -->
      <aside
        v-if="showLayers && preview"
        class="w-60 shrink-0 border-r border-[var(--border-color)] ui-surface-list flex flex-col overflow-hidden"
      >
        <!-- Pages -->
        <div class="border-b border-[var(--border-color)] p-2">
          <div class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] px-1 pb-1.5">
            Pages ({{ preview.pages.length }})
          </div>
          <div class="flex flex-col gap-0.5 max-h-36 overflow-y-auto">
            <button
              v-for="page in preview.pages"
              :key="page.id"
              type="button"
              class="w-full text-left px-2 py-1 text-[10px] truncate border transition-colors cursor-pointer"
              :class="
                preview.selected_node_id === page.id || preview.active_page_id === page.id
                  ? 'bg-[var(--accent-color)]/15 border-[var(--accent-color)]/60 text-[var(--accent-color)] font-bold'
                  : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--border-color)]'
              "
              @click="selectNode(page.id)"
            >
              {{ page.name }}
            </button>
          </div>
        </div>

        <!-- Layer Tree -->
        <div class="flex-1 min-h-0 flex flex-col p-2 overflow-hidden">
          <div class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] px-1 pb-1.5 shrink-0">
            Layers ({{ preview.layers.length }})
          </div>
          <div class="flex-1 overflow-y-auto flex flex-col gap-0.5">
            <button
              v-for="layer in preview.layers"
              :key="layer.id"
              type="button"
              class="w-full text-left py-1 pr-2 text-[10px] flex items-center justify-between gap-2 border transition-colors cursor-pointer"
              :style="{ paddingLeft: `${8 + layer.depth * 12}px` }"
              :class="
                preview.selected_node_id === layer.id
                  ? 'bg-[var(--accent-color)]/15 border-[var(--accent-color)]/60 text-[var(--accent-color)] font-bold'
                  : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--border-color)]'
              "
              @click="selectNode(layer.id)"
            >
              <span class="truncate">{{ layer.name || layer.kind }}</span>
              <span class="text-[8px] uppercase opacity-60 shrink-0">{{ layer.kind }}</span>
            </button>
          </div>
        </div>
      </aside>

      <!-- Scrollable Vector Viewport -->
      <div
        v-if="preview"
        ref="viewportRef"
        class="flex-1 min-h-0 relative overflow-auto bg-[var(--bg-color)] p-6"
        :class="fitMode === 'free' ? (isPanning ? 'cursor-grabbing select-none' : 'cursor-grab select-none') : 'cursor-default'"
        @wheel="handleWheel"
        @pointerdown="handlePointerDown"
        @pointermove="handlePointerMove"
        @pointerup="handlePointerUp"
        @pointercancel="handlePointerUp"
      >
        <div class="mx-auto relative" :style="scaledStageStyle">
          <div
            ref="svgContainerRef"
            class="absolute top-0 left-0 [&_svg]:block [&_svg]:shadow-2xl [&_svg]:border [&_svg]:border-[var(--border-color)]"
            :style="svgScaleStyle"
            v-html="preview.svg"
          />
        </div>
      </div>

      <!-- Empty State -->
      <AppEmptyState
        v-else
        :icon="Layers"
        title="No design prototype yet"
      >
        <template #description>
          Ask an agent to create a prototype using the Designer MCP, or open One-Ship Designer to sketch your screens in
          <code class="text-[var(--fg-color)]">{{ status?.design_dir_path ?? 'design/' }}</code>.
        </template>
        <template #actions>
          <AppTooltip text="Open One-Ship Designer">
            <button
              type="button"
              class="ui-icon-btn disabled:opacity-50"
              :disabled="isLaunching"
              @click="handleOpenEditor"
            >
              <ExternalLink :size="14" />
            </button>
          </AppTooltip>
        </template>
      </AppEmptyState>
    </div>
  </div>
</template>
