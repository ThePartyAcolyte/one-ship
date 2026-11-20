<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, computed, nextTick } from 'vue';
import {
  CircuitBoard,
  ExternalLink,
  Loader2,
  PanelLeft,
  Plus,
  ZoomIn,
  ZoomOut,
  Maximize2,
} from '@lucide/vue';
import { AppTooltip, AppSelect, AppInput, AppToggle, AppEmptyState } from '../ui';
import { daemonCall } from '../../utils/daemonClient';
import { useHardwarePreviewStore, pcbPreviewKey } from '../../stores/hardwarePreviewStore';
import type { ProjectRecord } from '../../types';
import type { PcbcraftStatus } from '../../types/generated/PcbcraftStatus';
import type { PcbcraftProjectInspection } from '../../types/generated/PcbcraftProjectInspection';
import type { PcbcraftSvgPreviewResult } from '../../types/generated/PcbcraftSvgPreviewResult';

type FitMode = 'fit-all' | 'fit-width' | 'free';
type PreviewMode = 'schematic' | 'board';
type SidebarTab = 'bom' | 'nets' | 'drc';

const MODE_OPTIONS = [
  { value: 'schematic', label: 'Schematic' },
  { value: 'board', label: 'Board' },
];

const props = defineProps<{
  project: ProjectRecord;
}>();

const hardwarePreviewStore = useHardwarePreviewStore();

const status = ref<PcbcraftStatus | null>(null);
const selectedTarget = ref<string>('');
const selectedMode = ref<PreviewMode>('schematic');
const selectedSheetId = ref<string>('');
const inspection = ref<PcbcraftProjectInspection | null>(null);
const preview = ref<PcbcraftSvgPreviewResult | null>(null);
const isLoading = ref(false);
const isLaunching = ref(false);
const showSidebar = ref(true);
const activeSidebarTab = ref<SidebarTab>('bom');
const bomFilter = ref('');
const errorMessage = ref<string | null>(null);
const pendingReviewTarget = ref<string | null>(null);
const isRevertingSnapshot = ref(false);

const showCreateModal = ref(false);
const newTargetName = ref('');
const openAfterCreate = ref(true);
const isCreatingTarget = ref(false);
const createError = ref<string | null>(null);

const viewportRef = ref<HTMLElement | null>(null);
const svgContainerRef = ref<HTMLElement | null>(null);
const svgNaturalWidth = ref(1000);
const svgNaturalHeight = ref(700);
const fitMode = ref<FitMode>('fit-all');
const zoom = ref(1);
const isPanning = ref(false);
let panStartX = 0;
let panStartY = 0;
let panScrollLeft = 0;
let panScrollTop = 0;

let suppressSelectionWatch = false;
let renderSeq = 0;
let resizeObserver: ResizeObserver | null = null;

const targetOptions = computed(() =>
  (status.value?.pcb_targets ?? []).map((t) => ({ value: t, label: t }))
);

const sheetOptions = computed(() =>
  (inspection.value?.sheets ?? []).map((s) => ({ value: s.id, label: s.name }))
);

const filteredComponents = computed(() => {
  const list = inspection.value?.components ?? [];
  const q = bomFilter.value.trim().toLowerCase();
  if (!q) return list;
  return list.filter(
    (c) =>
      c.reference.toLowerCase().includes(q) ||
      c.value.toLowerCase().includes(q) ||
      c.definition.toLowerCase().includes(q) ||
      (c.footprint ?? '').toLowerCase().includes(q)
  );
});

const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`);

const fitModeLabel = computed(() => {
  if (fitMode.value === 'fit-all') return 'Fit All';
  if (fitMode.value === 'fit-width') return 'Fit Width';
  return 'Free';
});

const scaledStageStyle = computed(() => ({
  width: `${Math.max(1, Math.round(svgNaturalWidth.value * zoom.value))}px`,
  height: `${Math.max(1, Math.round(svgNaturalHeight.value * zoom.value))}px`,
}));

const svgScaleStyle = computed(() => ({
  width: `${svgNaturalWidth.value}px`,
  height: `${svgNaturalHeight.value}px`,
  transform: `scale(${zoom.value})`,
  transformOrigin: 'top left',
}));

function measureSvgDimensions() {
  const container = svgContainerRef.value;
  if (!container) return;
  const svgEl = container.querySelector('svg');
  if (!svgEl) return;

  const vb = svgEl.viewBox?.baseVal;
  const vbW = vb && vb.width > 0 ? vb.width : 100;
  const vbH = vb && vb.height > 0 ? vb.height : 70;
  const baseW = 1000;
  const baseH = Math.max(300, Math.min(2400, Math.round(baseW * (vbH / vbW))));

  svgNaturalWidth.value = baseW;
  svgNaturalHeight.value = baseH;
}

function applyFitMode() {
  if (fitMode.value === 'free') return;
  const vp = viewportRef.value;
  if (!vp || !svgNaturalWidth.value || !svgNaturalHeight.value) return;

  const availW = Math.max(100, vp.clientWidth - 48);
  const availH = Math.max(100, vp.clientHeight - 48);
  const nextScale =
    fitMode.value === 'fit-width'
      ? availW / svgNaturalWidth.value
      : Math.min(availW / svgNaturalWidth.value, availH / svgNaturalHeight.value);

  zoom.value = Math.min(4, Math.max(0.05, Number(nextScale.toFixed(3))));
}

function cycleFitMode() {
  if (fitMode.value === 'fit-all') {
    fitMode.value = 'fit-width';
  } else if (fitMode.value === 'fit-width') {
    fitMode.value = 'free';
  } else {
    fitMode.value = 'fit-all';
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

async function refreshInspectionAndPreview(resetScroll = false, force = false) {
  if (!props.project?.path || !selectedTarget.value) return;
  const projectPath = props.project.path;
  const target = selectedTarget.value;
  const cache = hardwarePreviewStore.getPcbCache(projectPath);

  cache.selectedTarget = target;
  cache.selectedMode = selectedMode.value;
  cache.selectedSheetId = selectedSheetId.value;

  const isDirty = force || Boolean(cache.dirtyTargets[target]);
  const cachedInspect = !isDirty ? (cache.inspectionsByTarget[target] ?? null) : null;
  const seq = ++renderSeq;
  errorMessage.value = null;

  try {
    let nextInspect = cachedInspect;
    if (!nextInspect) {
      isLoading.value = true;
      nextInspect = await daemonCall('inspect_pcbcraft_project', {
        project_path: projectPath,
        target_name: target,
      });
      if (seq !== renderSeq) return;
      hardwarePreviewStore.savePcbInspection(projectPath, target, nextInspect);
    }
    inspection.value = nextInspect;

    let adjusted = false;
    if (nextInspect.sheets.length > 0 && !nextInspect.sheets.some((s) => s.id === selectedSheetId.value)) {
      suppressSelectionWatch = true;
      selectedSheetId.value = nextInspect.sheets[0].id;
      adjusted = true;
    }
    if (selectedMode.value === 'board' && nextInspect.boards.length === 0) {
      suppressSelectionWatch = true;
      selectedMode.value = 'schematic';
      adjusted = true;
    }
    cache.selectedMode = selectedMode.value;
    cache.selectedSheetId = selectedSheetId.value;
    if (adjusted) {
      await nextTick();
      suppressSelectionWatch = false;
      if (seq !== renderSeq) return;
    }

    const effectiveSheetId =
      selectedMode.value === 'schematic' && selectedSheetId.value ? selectedSheetId.value : '';
    const key = pcbPreviewKey(target, selectedMode.value, effectiveSheetId);
    const cachedPreview = !isDirty ? (cache.previewsByKey[key] ?? null) : null;

    let nextPreview = cachedPreview;
    if (!nextPreview) {
      isLoading.value = true;
      nextPreview = await daemonCall('render_pcbcraft_svg_preview', {
        project_path: projectPath,
        target_name: target,
        sheet_id: effectiveSheetId || null,
        mode: selectedMode.value,
      });
      if (seq !== renderSeq) return;
      hardwarePreviewStore.savePcbPreview(
        projectPath,
        target,
        selectedMode.value,
        effectiveSheetId,
        nextPreview
      );
    }

    preview.value = nextPreview;
    await nextTick();
    setupViewportObserver();
    measureSvgDimensions();
    applyFitMode();
    if (resetScroll && viewportRef.value) {
      viewportRef.value.scrollTop = 0;
      viewportRef.value.scrollLeft = 0;
    }
  } catch (err: unknown) {
    if (seq === renderSeq) {
      errorMessage.value = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (seq === renderSeq) {
      isLoading.value = false;
    }
  }
}

async function loadStatusAndInspect(forceRefresh = false, resetScroll = false) {
  if (!props.project?.path) return;
  const projectPath = props.project.path;
  const cache = hardwarePreviewStore.getPcbCache(projectPath);
  const hasDirty = Object.keys(cache.dirtyTargets).length > 0;

  // Serve directly from Pinia cache when already loaded and unmodified (0ms tab switch).
  if (!forceRefresh && cache.status && !hasDirty) {
    suppressSelectionWatch = true;
    status.value = cache.status;
    if (cache.status.pcb_targets.length === 0) {
      selectedTarget.value = '';
      selectedSheetId.value = '';
      inspection.value = null;
      preview.value = null;
      await nextTick();
      suppressSelectionWatch = false;
      return;
    }

    selectedTarget.value =
      cache.selectedTarget && cache.status.pcb_targets.includes(cache.selectedTarget)
        ? cache.selectedTarget
        : cache.status.pcb_targets[0];
    selectedMode.value = cache.selectedMode || 'schematic';
    selectedSheetId.value = cache.selectedSheetId || '';
    await nextTick();
    suppressSelectionWatch = false;

    await refreshInspectionAndPreview(resetScroll, false);
    return;
  }

  try {
    const nextStatus = await daemonCall('get_project_pcbcraft_status', {
      project_path: projectPath,
    });
    cache.status = nextStatus;
    status.value = nextStatus;

    if (nextStatus.pcb_targets.length === 0) {
      suppressSelectionWatch = true;
      selectedTarget.value = '';
      selectedSheetId.value = '';
      inspection.value = null;
      preview.value = null;
      await nextTick();
      suppressSelectionWatch = false;
      return;
    }

    const preferredTarget = cache.selectedTarget || selectedTarget.value;
    const nextTarget =
      preferredTarget && nextStatus.pcb_targets.includes(preferredTarget)
        ? preferredTarget
        : nextStatus.pcb_targets[0];

    suppressSelectionWatch = true;
    selectedTarget.value = nextTarget;
    selectedMode.value = cache.selectedMode || selectedMode.value;
    selectedSheetId.value = cache.selectedSheetId || selectedSheetId.value;
    await nextTick();
    suppressSelectionWatch = false;

    await refreshInspectionAndPreview(resetScroll, forceRefresh);
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  }
}

async function handleOpenEditor() {
  if (!props.project?.path || isLaunching.value) return;
  isLaunching.value = true;
  errorMessage.value = null;
  try {
    const nextStatus = await daemonCall('launch_project_pcbcraft', {
      project_path: props.project.path,
      target_name: selectedTarget.value || null,
    });
    hardwarePreviewStore.getPcbCache(props.project.path).status = nextStatus;
    status.value = nextStatus;
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLaunching.value = false;
  }
}

function openCreateModal() {
  createError.value = null;
  const count = status.value?.pcb_targets.length ?? 0;
  newTargetName.value = count === 0 ? 'main-board' : `board-${count + 1}`;
  openAfterCreate.value = true;
  showCreateModal.value = true;
}

function closeCreateModal() {
  if (isCreatingTarget.value) return;
  showCreateModal.value = false;
  createError.value = null;
}

async function submitCreateTarget() {
  if (!props.project?.path || isCreatingTarget.value) return;
  const cleanName = newTargetName.value.trim();
  if (!cleanName) {
    createError.value = 'Enter a project folder name.';
    return;
  }

  isCreatingTarget.value = true;
  createError.value = null;
  try {
    const nextStatus = await daemonCall('create_pcbcraft_project', {
      project_path: props.project.path,
      target_name: cleanName,
    });
    hardwarePreviewStore.getPcbCache(props.project.path).status = nextStatus;
    status.value = nextStatus;
    suppressSelectionWatch = true;
    selectedTarget.value = nextStatus.pcb_targets.includes(cleanName)
      ? cleanName
      : (nextStatus.pcb_targets[0] ?? '');
    selectedSheetId.value = '';
    await nextTick();
    suppressSelectionWatch = false;
    showCreateModal.value = false;
    await refreshInspectionAndPreview(true, true);

    if (openAfterCreate.value && selectedTarget.value) {
      await handleOpenEditor();
    }
  } catch (err: unknown) {
    createError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isCreatingTarget.value = false;
  }
}

watch(
  () => props.project?.path,
  () => {
    inspection.value = null;
    preview.value = null;
    void loadStatusAndInspect(false, true);
  }
);

watch(selectedTarget, (next, prev) => {
  if (suppressSelectionWatch) return;
  if (next && next !== prev) {
    suppressSelectionWatch = true;
    selectedSheetId.value = '';
    void nextTick(() => {
      suppressSelectionWatch = false;
      void refreshInspectionAndPreview(true, false);
    });
  }
});

watch([selectedMode, selectedSheetId], () => {
  if (suppressSelectionWatch) return;
  if (selectedTarget.value) {
    void refreshInspectionAndPreview(false, false);
  }
});

async function handleRevertLatestSnapshot() {
  if (!props.project?.id || isRevertingSnapshot.value) return;
  const target = pendingReviewTarget.value || selectedTarget.value;
  if (!target) return;
  isRevertingSnapshot.value = true;
  errorMessage.value = null;
  try {
    const list = await daemonCall('list_project_snapshots', {
      projectId: props.project.id,
      conversationId: null,
      agentId: null,
      limitCount: 30,
    });
    const match = list.find(
      (s) => s.domain === 'pcbcraft' && (!s.asset_key || s.asset_key === target)
    );
    if (!match) {
      errorMessage.value = `No PCBCraft snapshot found to revert for '${target}'.`;
      return;
    }
    await daemonCall('restore_project_snapshot', {
      projectId: props.project.id,
      snapshotId: match.id,
      files: null,
    });
    pendingReviewTarget.value = null;
    await loadStatusAndInspect(true, false);
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isRevertingSnapshot.value = false;
  }
}

watch(
  () => (props.project?.path ? hardwarePreviewStore.getPcbCache(props.project.path).revision : 0),
  (nextRev, prevRev) => {
    if (nextRev > 0 && nextRev !== prevRev) {
      if (selectedTarget.value) {
        pendingReviewTarget.value = selectedTarget.value;
      }
      void loadStatusAndInspect(false, false);
    }
  }
);

function handleContextRefresh(e: Event) {
  const tab = (e as CustomEvent<{ tab?: string }>).detail?.tab;
  if (tab === 'pcb' && selectedTarget.value) {
    void loadStatusAndInspect(true, false);
  }
}

onMounted(() => {
  window.addEventListener('oneship:context-refresh', handleContextRefresh);
  void loadStatusAndInspect(false, true);
});

onUnmounted(() => {
  window.removeEventListener('oneship:context-refresh', handleContextRefresh);
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});
</script>

<template>
  <div class="h-full flex flex-col bg-[var(--bg-color)] text-[var(--fg-color)] font-mono overflow-hidden">
    <!-- Top Toolbar -->
    <div class="h-9 px-3 ui-surface-title border-b border-[var(--border-color)] flex items-center justify-between gap-2 shrink-0">
      <div class="flex items-center gap-2 min-w-0 overflow-x-auto scrollbar-none">
        <AppTooltip text="Toggle BOM, nets, and DRC sidebar">
          <button
            type="button"
            class="ui-icon-btn"
            :class="{ '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15': showSidebar }"
            @click="showSidebar = !showSidebar"
          >
            <PanelLeft class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <div v-if="targetOptions.length > 0" class="w-44 shrink-0">
          <AppSelect
            v-model="selectedTarget"
            :options="targetOptions"
            placeholder="Select PCB project..."
            compact
            class="[&>button]:h-6"
          />
        </div>
        <span v-else class="text-[10px] text-[var(--dim-color)] whitespace-nowrap">
          No PCB files in {{ status?.pcb_dir_path ?? 'pcb/' }}
        </span>

        <AppTooltip text="New PCB project">
          <button
            type="button"
            class="ui-icon-btn disabled:opacity-50"
            :disabled="isCreatingTarget"
            @click="openCreateModal"
          >
            <Plus class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <div v-if="targetOptions.length > 0" class="w-32 shrink-0">
          <AppSelect
            v-model="selectedMode"
            :options="MODE_OPTIONS"
            compact
            class="[&>button]:h-6"
          />
        </div>

        <div v-if="selectedMode === 'schematic' && sheetOptions.length > 1" class="w-36 shrink-0">
          <AppSelect
            v-model="selectedSheetId"
            :options="sheetOptions"
            compact
            class="[&>button]:h-6"
          />
        </div>
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
          <AppTooltip text="Cycle view mode: Fit All / Fit Width / Free">
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

        <AppTooltip :text="status?.is_editor_running ? 'Focus PCBCraft' : 'Open PCBCraft'">
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

    <!-- Review / Snapshot Action Banner -->
    <div
      v-if="pendingReviewTarget"
      class="px-4 py-1.5 bg-[var(--accent-color)]/10 border-b border-[var(--accent-color)]/40 text-[11px] flex items-center justify-between gap-2"
    >
      <span>
        Updated <span class="font-bold text-[var(--accent-color)]">{{ pendingReviewTarget }}</span> on disk.
      </span>
      <div class="flex items-center gap-1.5 shrink-0">
        <button
          type="button"
          class="px-2 py-0.5 text-[10px] uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black cursor-pointer"
          @click="pendingReviewTarget = null; void handleOpenEditor();"
        >
          Open in PCBCraft
        </button>
        <button
          type="button"
          class="px-2 py-0.5 text-[10px] uppercase border border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:bg-[var(--status-warn-bg)] cursor-pointer disabled:opacity-50"
          :disabled="isRevertingSnapshot"
          @click="handleRevertLatestSnapshot"
        >
          {{ isRevertingSnapshot ? 'Reverting...' : 'Revert Snapshot' }}
        </button>
        <button
          type="button"
          class="px-2 py-0.5 text-[10px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
          @click="pendingReviewTarget = null"
        >
          Keep
        </button>
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

    <!-- Main Split: Sidebar & SVG Viewport -->
    <div class="flex-1 min-h-0 flex overflow-hidden">
      <aside
        v-if="showSidebar && selectedTarget"
        class="w-80 shrink-0 border-r border-[var(--border-color)] ui-surface-list flex flex-col overflow-hidden"
      >
        <!-- Summary Metrics & DRC Header -->
        <div class="p-3 border-b border-[var(--border-color)] ui-surface-title flex flex-col gap-2 shrink-0">
          <div class="flex items-center justify-between gap-2">
            <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] truncate">
              {{ inspection?.project_name || selectedTarget }}
            </span>
            <span
              v-if="inspection"
              class="px-1.5 py-0.5 text-[9px] uppercase font-bold border shrink-0"
              :class="
                inspection.drc_status === 'pass'
                  ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'
                  : inspection.drc_status === 'fail'
                    ? 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]'
                    : 'border-[var(--border-color)] text-[var(--dim-color)]'
              "
            >
              DRC: {{ inspection.drc_status === 'fail' ? `${inspection.drc_total_violations_count} FAIL` : inspection.drc_status.toUpperCase() }}
            </span>
          </div>

          <div v-if="inspection" class="grid grid-cols-4 gap-1.5 text-[10px]">
            <div class="border border-[var(--border-color)] p-1 text-center">
              <div class="text-[8px] uppercase text-[var(--dim-color)]">Sheets</div>
              <div class="font-bold tabular-nums">{{ inspection.sheets.length }}</div>
            </div>
            <div class="border border-[var(--border-color)] p-1 text-center">
              <div class="text-[8px] uppercase text-[var(--dim-color)]">Parts</div>
              <div class="font-bold tabular-nums">{{ inspection.components.length }}</div>
            </div>
            <div class="border border-[var(--border-color)] p-1 text-center">
              <div class="text-[8px] uppercase text-[var(--dim-color)]">Nets</div>
              <div class="font-bold tabular-nums">{{ inspection.nets.length }}</div>
            </div>
            <div class="border border-[var(--border-color)] p-1 text-center">
              <div class="text-[8px] uppercase text-[var(--dim-color)]">Tracks</div>
              <div class="font-bold tabular-nums">{{ inspection.boards[0]?.tracks_count ?? 0 }}</div>
            </div>
          </div>
        </div>

        <!-- Sidebar Sub-tabs -->
        <div class="grid grid-cols-3 border-b border-[var(--border-color)] ui-surface-title text-[9px] font-bold uppercase shrink-0">
          <button
            type="button"
            class="py-1.5 cursor-pointer border-r border-[var(--border-color)] transition-colors"
            :class="activeSidebarTab === 'bom' ? 'text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="activeSidebarTab = 'bom'"
          >
            BOM ({{ inspection?.components.length ?? 0 }})
          </button>
          <button
            type="button"
            class="py-1.5 cursor-pointer border-r border-[var(--border-color)] transition-colors"
            :class="activeSidebarTab === 'nets' ? 'text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="activeSidebarTab = 'nets'"
          >
            Nets ({{ inspection?.nets.length ?? 0 }})
          </button>
          <button
            type="button"
            class="py-1.5 cursor-pointer transition-colors"
            :class="activeSidebarTab === 'drc' ? 'text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="activeSidebarTab = 'drc'"
          >
            DRC ({{ inspection?.drc_total_violations_count ?? 0 }})
          </button>
        </div>

        <!-- BOM Tab -->
        <div v-if="activeSidebarTab === 'bom'" class="flex-1 min-h-0 flex flex-col overflow-hidden">
          <div class="p-2 border-b border-[var(--border-color)] shrink-0">
            <AppInput
              v-model="bomFilter"
              placeholder="Filter reference, value, footprint..."
              class="!h-6 !text-[10px]"
            />
          </div>
          <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-1.5">
            <div
              v-for="comp in filteredComponents"
              :key="comp.id"
              class="border border-[var(--border-color)] ui-surface-card p-2 flex flex-col gap-0.5 text-[10px] transition-colors"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="font-bold text-[var(--accent-color)]">{{ comp.reference }}</span>
                <span class="text-[var(--fg-color)] truncate">{{ comp.value }}</span>
              </div>
              <div class="flex items-center justify-between gap-2 text-[9px] text-[var(--dim-color)]">
                <span class="truncate">{{ comp.definition }}</span>
                <span v-if="comp.footprint" class="shrink-0 border border-[var(--border-color)] px-1">
                  {{ comp.footprint }}
                </span>
              </div>
              <div v-if="comp.exclude_from_bom || comp.exclude_from_simulation" class="flex gap-1 pt-0.5">
                <span v-if="comp.exclude_from_bom" class="text-[8px] px-1 border border-[var(--border-color)] text-[var(--dim-color)]">
                  NO BOM
                </span>
                <span v-if="comp.exclude_from_simulation" class="text-[8px] px-1 border border-[var(--border-color)] text-[var(--dim-color)]">
                  NO SIM
                </span>
              </div>
            </div>
            <div v-if="filteredComponents.length === 0" class="text-[11px] text-[var(--dim-color)] p-3 text-center">
              No components match.
            </div>
          </div>
        </div>

        <!-- Nets Tab -->
        <div v-else-if="activeSidebarTab === 'nets'" class="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
          <div
            v-for="net in inspection?.nets ?? []"
            :key="net.name"
            class="border border-[var(--border-color)] ui-surface-card px-2 py-1.5 text-[10px] flex items-center justify-between gap-2 transition-colors"
          >
            <span class="font-bold truncate text-[var(--fg-color)]">{{ net.name }}</span>
            <span class="text-[9px] text-[var(--dim-color)] tabular-nums shrink-0">
              {{ net.terminals_count }} pins · {{ net.junctions_count }} jct
            </span>
          </div>
          <div v-if="!inspection || inspection.nets.length === 0" class="text-[11px] text-[var(--dim-color)] p-3 text-center">
            No netlist nodes yet.
          </div>
        </div>

        <!-- DRC Tab -->
        <div v-else class="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1.5">
          <div v-if="inspection?.drc_status === 'no_board'" class="text-[11px] text-[var(--dim-color)] p-3">
            No physical board exists yet in this project.
          </div>
          <div v-else-if="inspection?.drc_status === 'pass'" class="p-3 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[11px]">
            Design rule check passed with 0 violations.
          </div>
          <div
            v-for="(v, idx) in inspection?.drc_violations ?? []"
            :key="`${v.code}-${idx}`"
            class="border border-[var(--status-error-border)] bg-[var(--status-error-bg)] p-2 text-[10px] flex flex-col gap-0.5"
          >
            <div class="font-bold text-[var(--status-error-color)] uppercase">
              [{{ v.severity }}] {{ v.code }}
            </div>
            <div class="text-[var(--fg-color)]">{{ v.message }}</div>
          </div>
        </div>
      </aside>

      <!-- Scrollable Vector SVG Viewport -->
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
            class="absolute top-0 left-0 [&_svg]:w-full [&_svg]:h-full [&_svg]:block [&_svg]:shadow-2xl [&_svg]:border [&_svg]:border-[var(--border-color)]"
            :style="svgScaleStyle"
            v-html="preview.svg"
          />
        </div>
        <div class="fixed bottom-4 right-6 px-2 py-0.5 text-[9px] uppercase border border-[var(--border-color)] ui-surface-card text-[var(--dim-color)] pointer-events-none">
          {{ preview.mode }} · {{ preview.title }} · {{ preview.elements_count }} els
        </div>
      </div>

      <!-- Empty State -->
      <AppEmptyState
        v-else
        :icon="CircuitBoard"
        title="No PCB projects yet"
      >
        <template #description>
          Create your first PCBCraft project in
          <code class="text-[var(--fg-color)]">{{ status?.pcb_dir_path ?? 'pcb/' }}</code>, or open PCBCraft externally.
        </template>
        <template #actions>
          <AppTooltip text="New PCB project">
            <button
              type="button"
              class="ui-icon-btn disabled:opacity-50"
              :disabled="isCreatingTarget"
              @click="openCreateModal"
            >
              <Plus :size="14" />
            </button>
          </AppTooltip>
          <AppTooltip text="Open PCBCraft">
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

    <!-- Create PCB Project Modal -->
    <div
      v-if="showCreateModal"
      class="fixed inset-0 z-50 flex items-center justify-center ui-overlay-backdrop p-4"
      @click.self="closeCreateModal"
      @keydown.esc="closeCreateModal"
    >
      <form
        class="w-full max-w-md border border-[var(--border-color)] bg-[var(--ui-bg-card)] p-4 flex flex-col gap-3 shadow-2xl"
        @submit.prevent="submitCreateTarget"
      >
        <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
          <span class="text-xs font-bold uppercase tracking-wider">New PCB Project</span>
          <span class="text-[10px] text-[var(--dim-color)] truncate max-w-[220px]">
            {{ status?.pcb_dir_path ?? 'pcb/' }}
          </span>
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)]">Folder Name</label>
          <AppInput
            v-model="newTargetName"
            placeholder="main-board"
          />
        </div>

        <AppToggle
          v-model="openAfterCreate"
          label="Open in PCBCraft"
          description="Launch PCBCraft scoped to the pcb/ workspace after creation"
        />

        <div
          v-if="createError"
          class="px-2.5 py-1.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px]"
        >
          {{ createError }}
        </div>

        <div class="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border-color)]">
          <button
            type="button"
            class="px-3 py-1 text-[10px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
            :disabled="isCreatingTarget"
            @click="closeCreateModal"
          >
            Cancel
          </button>
          <button
            type="submit"
            class="px-3 py-1 text-[10px] uppercase font-bold border border-[var(--accent-color)] bg-[var(--accent-color)] text-[var(--bg-color)] hover:opacity-90 cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
            :disabled="isCreatingTarget"
          >
            <Loader2 v-if="isCreatingTarget" class="w-3 h-3 animate-spin" />
            <span>{{ openAfterCreate ? 'Create & Open' : 'Create' }}</span>
          </button>
        </div>
      </form>
    </div>
  </div>
</template>
