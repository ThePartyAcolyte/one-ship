<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, computed, nextTick } from 'vue';
import {
  Box,
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
import { useHardwarePreviewStore, cadPreviewKey } from '../../stores/hardwarePreviewStore';
import type { ProjectRecord } from '../../types';
import type { SolidcraftStatus } from '../../types/generated/SolidcraftStatus';
import type { SolidcraftPreviewResult } from '../../types/generated/SolidcraftPreviewResult';
import type { SolidcraftMetricsResult } from '../../types/generated/SolidcraftMetricsResult';

type FitMode = 'fit-all' | 'fit-width' | 'free';

const VIEW_OPTIONS = [
  { value: 'iso', label: 'ISO' },
  { value: 'front', label: 'FRONT' },
  { value: 'back', label: 'BACK' },
  { value: 'top', label: 'TOP' },
  { value: 'bottom', label: 'BOTTOM' },
  { value: 'left', label: 'LEFT' },
  { value: 'right', label: 'RIGHT' },
];

const TEMPLATE_OPTIONS = [
  { value: 'blank_mm', label: 'Blank — Millimeters (mm)' },
  { value: 'xy_sketch_mm', label: 'XY Sketch — Millimeters (mm)' },
  { value: 'blank_in', label: 'Blank — Inches (in)' },
  { value: 'xy_sketch_in', label: 'XY Sketch — Inches (in)' },
  { value: 'blank_cm', label: 'Blank — Centimeters (cm)' },
  { value: 'blank_m', label: 'Blank — Meters (m)' },
  { value: 'blank_ft', label: 'Blank — Feet (ft)' },
];

const props = defineProps<{
  project: ProjectRecord;
}>();

const hardwarePreviewStore = useHardwarePreviewStore();

const status = ref<SolidcraftStatus | null>(null);
const selectedFile = ref<string>('');
const selectedView = ref<string>('iso');
const sectionInput = ref<string>('');
const appliedSection = ref<string>('');
const preview = ref<SolidcraftPreviewResult | null>(null);
const metrics = ref<SolidcraftMetricsResult | null>(null);
const isLoadingPreview = ref(false);
const isLoadingMetrics = ref(false);
const isLaunching = ref(false);
const showMetricsSidebar = ref(true);
const errorMessage = ref<string | null>(null);

const showCreateModal = ref(false);
const newFileName = ref('');
const newTemplateId = ref('blank_mm');
const openAfterCreate = ref(true);
const isCreatingFile = ref(false);
const createError = ref<string | null>(null);

const viewportRef = ref<HTMLElement | null>(null);
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

const fileOptions = computed(() => {
  const files = status.value?.cad_files ?? [];
  return files.map((f) => ({ value: f, label: f }));
});

const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`);

const fitModeLabel = computed(() => {
  if (fitMode.value === 'fit-all') return 'Fit All';
  if (fitMode.value === 'fit-width') return 'Fit Width';
  return 'Free';
});

const imageNaturalWidth = computed(() => preview.value?.width_px || 1280);
const imageNaturalHeight = computed(() => preview.value?.height_px || 800);

const scaledStageStyle = computed(() => ({
  width: `${Math.max(1, Math.round(imageNaturalWidth.value * zoom.value))}px`,
  height: `${Math.max(1, Math.round(imageNaturalHeight.value * zoom.value))}px`,
}));

const scaledImageStyle = computed(() => ({
  width: `${imageNaturalWidth.value}px`,
  height: `${imageNaturalHeight.value}px`,
  transform: `scale(${zoom.value})`,
  transformOrigin: 'top left',
}));

function formatNum(val: number, decimals = 2): string {
  if (!Number.isFinite(val)) return '0';
  return val.toFixed(decimals);
}

function formatDimensions(minMm: [number, number, number], maxMm: [number, number, number]): string {
  const dx = Math.abs(maxMm[0] - minMm[0]);
  const dy = Math.abs(maxMm[1] - minMm[1]);
  const dz = Math.abs(maxMm[2] - minMm[2]);
  return `${formatNum(dx, 1)} × ${formatNum(dy, 1)} × ${formatNum(dz, 1)} mm`;
}

function applyFitMode() {
  if (fitMode.value === 'free') return;
  const vp = viewportRef.value;
  if (!vp || !imageNaturalWidth.value || !imageNaturalHeight.value) return;

  const availW = Math.max(100, vp.clientWidth - 48);
  const availH = Math.max(100, vp.clientHeight - 48);
  const nextScale =
    fitMode.value === 'fit-width'
      ? availW / imageNaturalWidth.value
      : Math.min(availW / imageNaturalWidth.value, availH / imageNaturalHeight.value);

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

function canEvaluateMetrics(fileName: string): boolean {
  const lower = fileName.toLowerCase();
  return lower.endsWith('.sc') || lower.endsWith('.step') || lower.endsWith('.stp');
}

async function refreshPreviewAndMetrics(resetScroll = false, force = false) {
  if (!props.project?.path || !selectedFile.value) return;
  const projectPath = props.project.path;
  const targetFile = selectedFile.value;
  const view = selectedView.value;
  const section = appliedSection.value.trim();

  const cache = hardwarePreviewStore.getCadCache(projectPath);
  cache.selectedFile = targetFile;
  cache.selectedView = view;
  cache.appliedSection = section;

  const isDirty = force || Boolean(cache.dirtyFiles[targetFile]);
  const key = cadPreviewKey(targetFile, view, section);
  const cachedPreview = !isDirty ? (cache.previewsByKey[key] ?? null) : null;
  const needsMetrics = canEvaluateMetrics(targetFile);
  const cachedMetrics = needsMetrics && !isDirty ? (cache.metricsByFile[targetFile] ?? null) : null;

  if (cachedPreview) {
    preview.value = cachedPreview;
  }
  if (needsMetrics && cachedMetrics) {
    metrics.value = cachedMetrics;
  } else if (!needsMetrics) {
    metrics.value = null;
  }

  const fetchPreview = isDirty || !cachedPreview;
  const fetchMetrics = needsMetrics && (isDirty || !cachedMetrics);

  if (!fetchPreview && !fetchMetrics) {
    errorMessage.value = null;
    await nextTick();
    setupViewportObserver();
    applyFitMode();
    if (resetScroll && viewportRef.value) {
      viewportRef.value.scrollTop = 0;
      viewportRef.value.scrollLeft = 0;
    }
    return;
  }

  const seq = ++renderSeq;
  errorMessage.value = null;
  if (fetchPreview) isLoadingPreview.value = true;
  if (fetchMetrics) isLoadingMetrics.value = true;

  const previewPromise = fetchPreview
    ? daemonCall('render_solidcraft_preview', {
        project_path: projectPath,
        file_name: targetFile,
        view,
        section: section || null,
      })
        .then((res) => {
          hardwarePreviewStore.saveCadPreview(projectPath, targetFile, view, section, res);
          if (seq === renderSeq) {
            preview.value = res;
          }
          return true;
        })
        .catch((err: unknown) => {
          if (seq === renderSeq) {
            errorMessage.value = err instanceof Error ? err.message : String(err);
          }
          return false;
        })
    : Promise.resolve(false);

  const metricsPromise = fetchMetrics
    ? daemonCall('inspect_solidcraft_metrics', {
        project_path: projectPath,
        file_name: targetFile,
      })
        .then((res) => {
          hardwarePreviewStore.saveCadMetrics(projectPath, targetFile, res);
          if (seq === renderSeq) {
            metrics.value = res;
          }
        })
        .catch((err: unknown) => {
          if (seq === renderSeq) {
            metrics.value = null;
            if (!errorMessage.value) {
              errorMessage.value = err instanceof Error ? err.message : String(err);
            }
          }
        })
    : Promise.resolve();

  try {
    const [previewUpdated] = await Promise.all([previewPromise, metricsPromise]);
    if (previewUpdated && seq === renderSeq) {
      await nextTick();
      setupViewportObserver();
      applyFitMode();
      if (resetScroll && viewportRef.value) {
        viewportRef.value.scrollTop = 0;
        viewportRef.value.scrollLeft = 0;
      }
    }
  } finally {
    if (seq === renderSeq) {
      isLoadingPreview.value = false;
      isLoadingMetrics.value = false;
    }
  }
}

async function loadStatusAndPreview(forceRefresh = false, resetScroll = false) {
  if (!props.project?.path) return;
  const projectPath = props.project.path;
  const cache = hardwarePreviewStore.getCadCache(projectPath);
  const hasDirty = Object.keys(cache.dirtyFiles).length > 0;

  // Serve directly from Pinia cache when already loaded and unmodified (0ms tab switch).
  if (!forceRefresh && cache.status && !hasDirty) {
    suppressSelectionWatch = true;
    status.value = cache.status;
    if (cache.status.cad_files.length === 0) {
      selectedFile.value = '';
      preview.value = null;
      metrics.value = null;
      await nextTick();
      suppressSelectionWatch = false;
      return;
    }
    const targetFile =
      cache.selectedFile && cache.status.cad_files.includes(cache.selectedFile)
        ? cache.selectedFile
        : cache.status.cad_files[0];
    selectedFile.value = targetFile;
    selectedView.value = cache.selectedView || 'iso';
    appliedSection.value = cache.appliedSection || '';
    sectionInput.value = cache.appliedSection || '';
    await nextTick();
    suppressSelectionWatch = false;

    await refreshPreviewAndMetrics(resetScroll, false);
    return;
  }

  try {
    const nextStatus = await daemonCall('get_project_solidcraft_status', {
      project_path: projectPath,
    });
    cache.status = nextStatus;
    status.value = nextStatus;

    if (nextStatus.cad_files.length === 0) {
      suppressSelectionWatch = true;
      selectedFile.value = '';
      preview.value = null;
      metrics.value = null;
      await nextTick();
      suppressSelectionWatch = false;
      return;
    }

    const preferredFile = cache.selectedFile || selectedFile.value;
    const nextFile =
      preferredFile && nextStatus.cad_files.includes(preferredFile)
        ? preferredFile
        : nextStatus.cad_files[0];

    suppressSelectionWatch = true;
    selectedFile.value = nextFile;
    if (cache.selectedView) selectedView.value = cache.selectedView;
    appliedSection.value = cache.appliedSection || appliedSection.value;
    sectionInput.value = appliedSection.value;
    await nextTick();
    suppressSelectionWatch = false;

    await refreshPreviewAndMetrics(resetScroll, forceRefresh);
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  }
}

function applySectionFilter() {
  appliedSection.value = sectionInput.value.trim();
  void refreshPreviewAndMetrics(false, false);
}

function clearSectionFilter() {
  sectionInput.value = '';
  appliedSection.value = '';
  void refreshPreviewAndMetrics(false, false);
}

async function handleOpenEditor() {
  if (!props.project?.path || isLaunching.value) return;
  isLaunching.value = true;
  errorMessage.value = null;
  try {
    const nextStatus = await daemonCall('launch_project_solidcraft', {
      project_path: props.project.path,
      file_name: selectedFile.value || null,
    });
    hardwarePreviewStore.getCadCache(props.project.path).status = nextStatus;
    status.value = nextStatus;
  } catch (err: unknown) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLaunching.value = false;
  }
}

function openCreateModal() {
  createError.value = null;
  const existingCount = status.value?.cad_files.length ?? 0;
  newFileName.value = existingCount === 0 ? 'Part1' : `Part${existingCount + 1}`;
  newTemplateId.value = 'blank_mm';
  openAfterCreate.value = true;
  showCreateModal.value = true;
}

function closeCreateModal() {
  if (isCreatingFile.value) return;
  showCreateModal.value = false;
  createError.value = null;
}

async function submitCreateFile() {
  if (!props.project?.path || isCreatingFile.value) return;
  const rawName = newFileName.value.trim();
  if (!rawName) {
    createError.value = 'Enter a file name.';
    return;
  }
  const expectedFileName = rawName.toLowerCase().endsWith('.sc') ? rawName : `${rawName}.sc`;

  isCreatingFile.value = true;
  createError.value = null;
  try {
    const prevFiles = new Set(status.value?.cad_files ?? []);
    const nextStatus = await daemonCall('create_solidcraft_file', {
      project_path: props.project.path,
      file_name: rawName,
      template_id: newTemplateId.value,
    });
    hardwarePreviewStore.getCadCache(props.project.path).status = nextStatus;
    status.value = nextStatus;
    const createdFile =
      nextStatus.cad_files.find((f) => f === expectedFileName) ??
      nextStatus.cad_files.find((f) => !prevFiles.has(f)) ??
      expectedFileName;

    showCreateModal.value = false;
    suppressSelectionWatch = true;
    selectedFile.value = createdFile;
    await nextTick();
    suppressSelectionWatch = false;
    await refreshPreviewAndMetrics(true, true);

    if (openAfterCreate.value) {
      isLaunching.value = true;
      try {
        const launchStatus = await daemonCall('launch_project_solidcraft', {
          project_path: props.project.path,
          file_name: createdFile,
        });
        hardwarePreviewStore.getCadCache(props.project.path).status = launchStatus;
        status.value = launchStatus;
      } finally {
        isLaunching.value = false;
      }
    }
  } catch (err: unknown) {
    createError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isCreatingFile.value = false;
  }
}

watch(() => props.project?.path, () => {
  preview.value = null;
  metrics.value = null;
  void loadStatusAndPreview(false, true);
});

watch(selectedFile, (next, prev) => {
  if (suppressSelectionWatch) return;
  if (next && next !== prev) {
    void refreshPreviewAndMetrics(true, false);
  }
});

watch(selectedView, (next, prev) => {
  if (suppressSelectionWatch) return;
  if (next !== prev && selectedFile.value) {
    void refreshPreviewAndMetrics(false, false);
  }
});

watch(
  () => (props.project?.path ? hardwarePreviewStore.getCadCache(props.project.path).revision : 0),
  (nextRev, prevRev) => {
    if (nextRev > 0 && nextRev !== prevRev) {
      void loadStatusAndPreview(false, false);
    }
  }
);

function handleContextRefresh(e: Event) {
  const tab = (e as CustomEvent<{ tab?: string }>).detail?.tab;
  if (tab === 'cad' && selectedFile.value) {
    void loadStatusAndPreview(true, false);
  }
}

onMounted(() => {
  window.addEventListener('oneship:context-refresh', handleContextRefresh);
  void loadStatusAndPreview(false, true);
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
        <AppTooltip text="Toggle physical metrics sidebar">
          <button
            type="button"
            class="ui-icon-btn"
            :class="{ '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15': showMetricsSidebar }"
            @click="showMetricsSidebar = !showMetricsSidebar"
          >
            <PanelLeft class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <div v-if="fileOptions.length > 0" class="w-44 shrink-0">
          <AppSelect
            v-model="selectedFile"
            :options="fileOptions"
            placeholder="Select CAD file..."
            compact
            class="[&>button]:h-6"
          />
        </div>
        <span v-else class="text-[10px] text-[var(--dim-color)] whitespace-nowrap">
          No CAD files in {{ status?.cad_dir_path ?? 'cad/' }}
        </span>

        <AppTooltip text="New CAD file">
          <button
            type="button"
            class="ui-icon-btn disabled:opacity-50"
            :disabled="isCreatingFile"
            @click="openCreateModal"
          >
            <Plus class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <div v-if="fileOptions.length > 0" class="w-28 shrink-0">
          <AppSelect
            v-model="selectedView"
            :options="VIEW_OPTIONS"
            compact
            class="[&>button]:h-6"
          />
        </div>

        <form v-if="fileOptions.length > 0" class="flex items-center gap-1 shrink-0" @submit.prevent="applySectionFilter">
          <div class="w-40">
            <AppInput
              :model-value="sectionInput"
              placeholder="Section (XY, z=10)"
              class="!h-6 !text-[10px]"
              @update:model-value="(val) => { sectionInput = String(val); }"
            />
          </div>
          <AppTooltip text="Apply section cut">
            <button
              type="submit"
              class="h-6 px-2 text-[9px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] cursor-pointer transition-colors"
            >
              Cut
            </button>
          </AppTooltip>
          <AppTooltip v-if="appliedSection" text="Clear section cut">
            <button
              type="button"
              class="h-6 px-2 text-[9px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer transition-colors"
              @click="clearSectionFilter"
            >
              Clear
            </button>
          </AppTooltip>
        </form>
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

        <AppTooltip :text="status?.is_editor_running ? 'Focus SolidCraft' : 'Open SolidCraft'">
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

    <!-- Main Split: Physical Metrics Sidebar & Snapshot Viewport -->
    <div class="flex-1 min-h-0 flex overflow-hidden">
      <aside
        v-if="showMetricsSidebar && selectedFile"
        class="w-72 shrink-0 border-r border-[var(--border-color)] ui-surface-list flex flex-col overflow-y-auto p-3 gap-4"
      >
        <!-- Summary Totals -->
        <div class="border border-[var(--border-color)] ui-surface-card p-2.5 flex flex-col gap-2">
          <div class="flex items-center justify-between">
            <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
              Model Metrics
            </span>
            <Loader2 v-if="isLoadingMetrics" class="w-3 h-3 animate-spin text-[var(--accent-color)]" />
          </div>

          <template v-if="metrics">
            <div class="grid grid-cols-2 gap-2 text-[11px]">
              <div class="border border-[var(--border-color)] p-1.5">
                <div class="text-[9px] uppercase text-[var(--dim-color)]">Bodies</div>
                <div class="font-bold tabular-nums">{{ metrics.bodies_count }}</div>
              </div>
              <div class="border border-[var(--border-color)] p-1.5">
                <div class="text-[9px] uppercase text-[var(--dim-color)]">Mass</div>
                <div class="font-bold tabular-nums">{{ formatNum(metrics.total_mass_g, 2) }} g</div>
              </div>
              <div class="border border-[var(--border-color)] p-1.5">
                <div class="text-[9px] uppercase text-[var(--dim-color)]">Volume</div>
                <div class="font-bold tabular-nums">{{ formatNum(metrics.total_volume_mm3, 1) }} mm³</div>
              </div>
              <div class="border border-[var(--border-color)] p-1.5">
                <div class="text-[9px] uppercase text-[var(--dim-color)]">Surface Area</div>
                <div class="font-bold tabular-nums">{{ formatNum(metrics.total_area_mm2, 1) }} mm²</div>
              </div>
            </div>
          </template>
          <div v-else class="text-[11px] text-[var(--dim-color)]">
            {{ canEvaluateMetrics(selectedFile) ? 'Evaluating geometry...' : 'Metrics available for .sc and .step files.' }}
          </div>
        </div>

        <!-- Bodies List -->
        <div v-if="metrics && metrics.bodies.length > 0" class="flex flex-col gap-2">
          <div class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
            Bodies ({{ metrics.bodies.length }})
          </div>
          <div
            v-for="body in metrics.bodies"
            :key="body.name"
            class="border border-[var(--border-color)] ui-surface-card p-2 flex flex-col gap-1 text-[10px] transition-colors"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="font-bold text-[11px] truncate text-[var(--fg-color)]">{{ body.name }}</span>
              <span
                class="px-1 border text-[9px] uppercase shrink-0"
                :class="body.closed
                  ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'
                  : 'border-[var(--border-color)] text-[var(--dim-color)]'"
              >
                {{ body.closed ? 'Solid' : 'Open' }}
              </span>
            </div>
            <div class="text-[var(--dim-color)]">
              {{ body.material }} · {{ formatDimensions(body.bbox_min_mm, body.bbox_max_mm) }}
            </div>
            <div class="flex items-center justify-between tabular-nums text-[var(--dim-color)]">
              <span>{{ formatNum(body.volume_mm3, 1) }} mm³</span>
              <span>{{ formatNum(body.mass_g, 2) }} g</span>
            </div>
            <div class="text-[9px] text-[var(--dim-color)] tabular-nums">
              F:{{ body.faces_count }} · E:{{ body.edges_count }} · V:{{ body.vertices_count }}
            </div>
          </div>
        </div>

        <!-- Parametric Variables -->
        <div v-if="metrics && metrics.params.length > 0" class="flex flex-col gap-1.5">
          <div class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
            Parameters ({{ metrics.params.length }})
          </div>
          <div
            v-for="param in metrics.params"
            :key="param.name"
            class="border border-[var(--border-color)] ui-surface-card px-2 py-1 text-[10px] flex items-center justify-between gap-2 transition-colors"
          >
            <span class="font-bold truncate">{{ param.name }}</span>
            <span class="text-[var(--dim-color)] truncate tabular-nums">
              {{ param.expression }}
              <template v-if="param.value !== null">
                ({{ formatNum(param.value, 2) }}{{ param.unit ? ` ${param.unit}` : '' }})
              </template>
            </span>
          </div>
        </div>
      </aside>

      <!-- Scrollable Snapshot Viewport -->
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
          <img
            :src="preview.png_data_url"
            :alt="`${preview.file_name} (${preview.view})`"
            class="absolute top-0 left-0 shadow-2xl border border-[var(--border-color)] pointer-events-none select-none"
            :style="scaledImageStyle"
          />
        </div>
        <div class="fixed bottom-4 right-6 px-2 py-0.5 text-[9px] uppercase border border-[var(--border-color)] ui-surface-card text-[var(--dim-color)] pointer-events-none">
          {{ preview.view }} · {{ preview.width_px }}×{{ preview.height_px }} · {{ preview.renderer }}
        </div>
      </div>

      <!-- Empty State -->
      <AppEmptyState
        v-else
        :icon="Box"
        title="No CAD files yet"
      >
        <template #description>
          Create your first <code class="text-[var(--fg-color)]">.sc</code> design in
          <code class="text-[var(--fg-color)]">{{ status?.cad_dir_path ?? 'cad/' }}</code>, or open SolidCraft externally.
        </template>
        <template #actions>
          <AppTooltip text="New CAD file">
            <button
              type="button"
              class="ui-icon-btn disabled:opacity-50"
              :disabled="isCreatingFile"
              @click="openCreateModal"
            >
              <Plus :size="14" />
            </button>
          </AppTooltip>
          <AppTooltip text="Open SolidCraft">
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

    <!-- Create CAD File Modal -->
    <div
      v-if="showCreateModal"
      class="fixed inset-0 z-50 flex items-center justify-center ui-overlay-backdrop p-4"
      @click.self="closeCreateModal"
      @keydown.esc="closeCreateModal"
    >
      <form
        class="w-full max-w-md border border-[var(--border-color)] bg-[var(--ui-bg-card)] p-4 flex flex-col gap-3 shadow-2xl"
        @submit.prevent="submitCreateFile"
      >
        <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
          <span class="text-xs font-bold uppercase tracking-wider">New CAD File</span>
          <span class="text-[10px] text-[var(--dim-color)] truncate max-w-[220px]">
            {{ status?.cad_dir_path ?? 'cad/' }}
          </span>
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)]">File Name</label>
          <AppInput
            v-model="newFileName"
            placeholder="Part1.sc"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)]">Template</label>
          <AppSelect
            v-model="newTemplateId"
            :options="TEMPLATE_OPTIONS"
          />
        </div>

        <AppToggle
          v-model="openAfterCreate"
          label="Open in SolidCraft"
          description="Launch the external editor directly on this file after creation"
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
            :disabled="isCreatingFile"
            @click="closeCreateModal"
          >
            Cancel
          </button>
          <button
            type="submit"
            class="px-3 py-1 text-[10px] uppercase font-bold border border-[var(--accent-color)] bg-[var(--accent-color)] text-[var(--bg-color)] hover:opacity-90 cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
            :disabled="isCreatingFile"
          >
            <Loader2 v-if="isCreatingFile" class="w-3 h-3 animate-spin" />
            <span>{{ openAfterCreate ? 'Create & Open' : 'Create' }}</span>
          </button>
        </div>
      </form>
    </div>
  </div>
</template>
