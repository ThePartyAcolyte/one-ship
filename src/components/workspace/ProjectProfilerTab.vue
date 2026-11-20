<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import {
  Activity,
  RotateCw,
  GitCompare,
  Loader2,
  ChevronRight,
  ExternalLink,
  X,
} from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { AppTooltip, AppSelect } from '../ui';
import { useProfilerStore } from '../../stores/useProfilerStore';
import { useCoreStore } from '../../stores/coreStore';
import type { ProjectRecord, TargetRunRecord, RunProfile, RunProcessProfile } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
}>();

const profilerStore = useProfilerStore();
const coreStore = useCoreStore();

const ALL_TARGETS = 'all';
const selectedTargetId = ref<string>(ALL_TARGETS);
const selectedRunId = ref<string | null>(null);
const isCompareMode = ref<boolean>(false);
const compareBeforeId = ref<string>('');
const compareAfterId = ref<string>('');

const projectRuns = computed<TargetRunRecord[]>(() => profilerStore.runs[props.project.path] ?? []);
const catalog = computed(() => profilerStore.catalogs[props.project.path] ?? null);

const targetOptions = computed(() => {
  const ids = new Set<string>(props.project.scripts.map((s) => s.id));
  for (const run of projectRuns.value) ids.add(run.target_id);
  return [
    { label: 'All services and tasks', value: ALL_TARGETS },
    ...[...ids].sort().map((id) => ({ label: id, value: id })),
  ];
});

const filteredRuns = computed(() =>
  selectedTargetId.value === ALL_TARGETS
    ? projectRuns.value
    : projectRuns.value.filter((r) => r.target_id === selectedTargetId.value)
);

const runOptions = computed(() =>
  filteredRuns.value.map((r) => ({ label: `${formatEpochMs(r.spawned_at_ms)} • ${r.target_id}`, value: r.run_id }))
);

const activeRun = computed(() => filteredRuns.value.find((r) => r.run_id === selectedRunId.value) ?? null);
const activeProfile = computed<RunProfile | null | undefined>(() =>
  selectedRunId.value ? profilerStore.profiles[selectedRunId.value] : undefined
);

function isRunning(run: TargetRunRecord): boolean {
  return run.terminated_at_ms === null;
}

function formatEpochMs(epochMs: number): string {
  return new Date(epochMs).toLocaleString([], {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

function formatDuration(run: TargetRunRecord): string {
  const endMs = run.terminated_at_ms ?? Date.now();
  const seconds = Math.max(0, (endMs - run.spawned_at_ms) / 1000);
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)}s`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}

function formatMbytes(value: number | null): string {
  return value === null ? '--' : `${value.toFixed(1)} MB`;
}

function formatDelta(value: number, unit: string): string {
  return `${value > 0 ? '+' : ''}${value.toFixed(1)}${unit}`;
}

/** Higher resource use after a change is a regression. */
function deltaClass(value: number): string {
  if (value > 0) return 'text-[var(--status-error-color)]';
  if (value < 0) return 'text-[var(--status-ok-color)]';
  return 'text-[var(--dim-color)]';
}

function statusClass(run: TargetRunRecord): string {
  if (isRunning(run)) return 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]';
  if (run.status === 'failed' || run.status === 'crashed') return 'border-[var(--status-error-border)] text-[var(--status-error-color)]';
  return 'border-[var(--border-color)] text-[var(--dim-color)]';
}

function profileMetrics(p: RunProfile): { label: string; value: string }[] {
  return [
    { label: 'AVG CPU', value: `${p.avg_cpu_percent.toFixed(1)}%` },
    { label: 'P50 CPU', value: `${p.p50_cpu_percent.toFixed(1)}%` },
    { label: 'P95 CPU', value: `${p.p95_cpu_percent.toFixed(1)}%` },
    { label: 'PEAK CPU', value: `${p.peak_cpu_percent.toFixed(1)}%` },
    { label: 'AVG RSS', value: formatMbytes(p.avg_rss_memory_mbytes) },
    { label: 'PEAK RSS', value: formatMbytes(p.peak_rss_memory_mbytes) },
    { label: 'PROCESSES', value: String(p.peak_processes_count) },
    { label: 'THREADS', value: p.peak_threads_count === null ? '--' : String(p.peak_threads_count) },
    { label: 'DISK READ', value: formatMbytes(p.disk_read_mbytes) },
    { label: 'DISK WRITE', value: formatMbytes(p.disk_written_mbytes) },
    { label: 'SAMPLED', value: `${p.sampled_seconds.toFixed(1)}s` },
    { label: 'TICKS', value: String(p.ticks_count) },
  ];
}

/** Process names of both runs of the comparison, joined by name. */
const comparedProcesses = computed(() => {
  const c = profilerStore.comparison;
  if (!c) return [];
  const before = new Map<string, RunProcessProfile>(c.before.processes.map((p) => [p.process_name, p]));
  const after = new Map<string, RunProcessProfile>(c.after.processes.map((p) => [p.process_name, p]));
  const names = [...new Set([...before.keys(), ...after.keys()])];
  return names
    .map((name) => {
      const b = before.get(name) ?? null;
      const a = after.get(name) ?? null;
      return {
        name,
        before: b,
        after: a,
        cpuDelta: (a?.avg_cpu_percent ?? 0) - (b?.avg_cpu_percent ?? 0),
        rssDelta: (a?.peak_rss_memory_mbytes ?? 0) - (b?.peak_rss_memory_mbytes ?? 0),
      };
    })
    .sort((x, y) => Math.abs(y.cpuDelta) - Math.abs(x.cpuDelta));
});

const comparedMetrics = computed(() => {
  const c = profilerStore.comparison;
  if (!c) return [];
  return [
    { label: 'SAMPLED', before: `${c.before.sampled_seconds.toFixed(1)}s`, after: `${c.after.sampled_seconds.toFixed(1)}s`, delta: c.deltas.sampled_seconds, unit: 's' },
    { label: 'AVG CPU', before: `${c.before.avg_cpu_percent.toFixed(1)}%`, after: `${c.after.avg_cpu_percent.toFixed(1)}%`, delta: c.deltas.avg_cpu_percent, unit: '%' },
    { label: 'P95 CPU', before: `${c.before.p95_cpu_percent.toFixed(1)}%`, after: `${c.after.p95_cpu_percent.toFixed(1)}%`, delta: c.deltas.p95_cpu_percent, unit: '%' },
    { label: 'PEAK CPU', before: `${c.before.peak_cpu_percent.toFixed(1)}%`, after: `${c.after.peak_cpu_percent.toFixed(1)}%`, delta: c.deltas.peak_cpu_percent, unit: '%' },
    { label: 'AVG RSS', before: formatMbytes(c.before.avg_rss_memory_mbytes), after: formatMbytes(c.after.avg_rss_memory_mbytes), delta: c.deltas.avg_rss_memory_mbytes, unit: ' MB' },
    { label: 'PEAK RSS', before: formatMbytes(c.before.peak_rss_memory_mbytes), after: formatMbytes(c.after.peak_rss_memory_mbytes), delta: c.deltas.peak_rss_memory_mbytes, unit: ' MB' },
  ];
});

async function selectRun(run: TargetRunRecord) {
  selectedRunId.value = run.run_id;
  await profilerStore.fetchProfile(run.run_id);
}

async function refresh() {
  profilerStore.clearError();
  await Promise.all([
    profilerStore.fetchRuns(props.project.path),
    profilerStore.fetchCatalog(props.project.path),
  ]);
  const current = filteredRuns.value.find((r) => r.run_id === selectedRunId.value) ?? filteredRuns.value[0];
  if (current) await selectRun(current);
}

async function openArtifact(run: TargetRunRecord) {
  if (!run.profile_artifact_path) return;
  try {
    await daemonCall('open_file_in_external_editor', { projectPath: props.project.path, relPath: run.profile_artifact_path });
  } catch (err) {
    profilerStore.lastError = `Open ${run.profile_artifact_path}: ${err instanceof Error ? err.message : String(err)}`;
  }
}

async function runComparison() {
  if (!compareBeforeId.value || !compareAfterId.value) return;
  profilerStore.clearError();
  await profilerStore.compare(compareBeforeId.value, compareAfterId.value);
}

watch(() => props.project.path, refresh);

watch(selectedTargetId, async () => {
  const first = filteredRuns.value[0];
  if (first) {
    await selectRun(first);
  } else {
    selectedRunId.value = null;
  }
});

// A service starting or stopping adds or closes a run.
watch(() => coreStore.runningServices.length, () => profilerStore.fetchRuns(props.project.path));

onMounted(refresh);
</script>

<template>
  <div class="flex-1 min-h-0 h-full flex flex-col overflow-hidden bg-[var(--bg-color)] select-text font-mono text-[11px]">

    <div class="h-9 shrink-0 border-b border-[var(--border-color)] px-3 flex items-center justify-between gap-3 ui-surface-title">
      <div class="flex items-center gap-2 min-w-0">
        <div class="w-48 sm:w-56 shrink-0">
          <AppSelect
            v-model="selectedTargetId"
            :options="targetOptions"
            compact
            class="[&>button]:h-6"
          />
        </div>
      </div>

      <div class="flex items-center gap-1.5 shrink-0">
        <AppTooltip :text="isCompareMode ? 'Back to run history' : 'Compare two runs'" position="bottom">
          <button
            type="button"
            @click="isCompareMode = !isCompareMode"
            class="h-6 px-2 text-[9px] uppercase border flex items-center gap-1 cursor-pointer transition-colors"
            :class="isCompareMode
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
          >
            <GitCompare class="w-3 h-3" />
            <span>{{ isCompareMode ? 'Runs' : 'Compare' }}</span>
          </button>
        </AppTooltip>
        <AppTooltip text="Refresh runs, profile and engines" position="bottom">
          <button
            type="button"
            @click="refresh"
            :disabled="profilerStore.isLoadingRuns"
            class="ui-icon-btn disabled:opacity-30"
          >
            <RotateCw class="w-3.5 h-3.5" :class="profilerStore.isLoadingRuns ? 'animate-spin' : ''" />
          </button>
        </AppTooltip>
      </div>
    </div>

    <div
      v-if="profilerStore.lastError"
      class="shrink-0 px-3 py-1.5 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px] flex items-center justify-between gap-2"
    >
      <span class="truncate">{{ profilerStore.lastError }}</span>
      <button class="shrink-0 cursor-pointer" @click="profilerStore.clearError()"><X class="w-3 h-3" /></button>
    </div>

    <!-- Comparison -->
    <div v-if="isCompareMode" class="flex-1 min-h-0 flex flex-col p-3 overflow-hidden gap-3">
      <div class="flex items-center gap-3 p-2 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 shrink-0">
        <div class="flex items-center gap-1.5 flex-1 min-w-0">
          <span class="text-[9px] text-[var(--dim-color)] shrink-0">BASELINE</span>
          <AppSelect v-model="compareBeforeId" :options="runOptions" placeholder="Run..." class="flex-1 min-w-0" />
        </div>
        <ChevronRight class="w-4 h-4 text-[var(--dim-color)] shrink-0" />
        <div class="flex items-center gap-1.5 flex-1 min-w-0">
          <span class="text-[9px] text-[var(--dim-color)] shrink-0">COMPARE</span>
          <AppSelect v-model="compareAfterId" :options="runOptions" placeholder="Run..." class="flex-1 min-w-0" />
        </div>
        <button
          @click="runComparison"
          :disabled="!compareBeforeId || !compareAfterId || profilerStore.isComparing"
          class="px-2.5 py-1 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)]/10 transition-colors cursor-pointer shrink-0 disabled:opacity-40"
        >
          <Loader2 v-if="profilerStore.isComparing" class="w-3.5 h-3.5 animate-spin" />
          <span v-else>DIFF</span>
        </button>
      </div>

      <div v-if="profilerStore.comparison" class="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3">
        <table class="w-full text-left border-collapse text-[10px] border border-[var(--border-color)]">
          <thead class="bg-[var(--ui-bg-card)] border-b border-[var(--border-color)]">
            <tr>
              <th class="p-2 font-semibold">METRIC</th>
              <th class="p-2 font-semibold text-right">BASELINE</th>
              <th class="p-2 font-semibold text-right">COMPARE</th>
              <th class="p-2 font-semibold text-right">DELTA</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="m in comparedMetrics" :key="m.label" class="border-b border-[var(--border-color)]/20">
              <td class="p-2 text-[var(--dim-color)]">{{ m.label }}</td>
              <td class="p-2 text-right">{{ m.before }}</td>
              <td class="p-2 text-right">{{ m.after }}</td>
              <td class="p-2 text-right font-bold" :class="deltaClass(m.delta)">{{ formatDelta(m.delta, m.unit) }}</td>
            </tr>
          </tbody>
        </table>

        <table class="w-full text-left border-collapse text-[10px] border border-[var(--border-color)]">
          <thead class="bg-[var(--ui-bg-card)] border-b border-[var(--border-color)]">
            <tr>
              <th class="p-2 font-semibold">PROCESS</th>
              <th class="p-2 font-semibold text-right">AVG CPU</th>
              <th class="p-2 font-semibold text-right">Δ CPU</th>
              <th class="p-2 font-semibold text-right">PEAK RSS</th>
              <th class="p-2 font-semibold text-right">Δ RSS</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="p in comparedProcesses" :key="p.name" class="border-b border-[var(--border-color)]/20">
              <td class="p-2 truncate max-w-xs">{{ p.name }}</td>
              <td class="p-2 text-right">{{ p.before ? `${p.before.avg_cpu_percent.toFixed(1)}%` : '--' }} → {{ p.after ? `${p.after.avg_cpu_percent.toFixed(1)}%` : '--' }}</td>
              <td class="p-2 text-right font-bold" :class="deltaClass(p.cpuDelta)">{{ formatDelta(p.cpuDelta, '%') }}</td>
              <td class="p-2 text-right">{{ formatMbytes(p.before?.peak_rss_memory_mbytes ?? null) }} → {{ formatMbytes(p.after?.peak_rss_memory_mbytes ?? null) }}</td>
              <td class="p-2 text-right font-bold" :class="deltaClass(p.rssDelta)">{{ formatDelta(p.rssDelta, ' MB') }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="flex-1 flex items-center justify-center text-[var(--dim-color)] italic opacity-60">
        Pick two runs and press DIFF.
      </div>
    </div>

    <!-- Run history -->
    <div v-else class="flex-1 min-h-0 grid grid-cols-1 lg:grid-cols-12 overflow-hidden">

      <div class="lg:col-span-4 border-r border-[var(--border-color)] flex flex-col min-h-0 h-full overflow-hidden bg-[var(--ui-bg-card)]/10">
        <div v-if="catalog" class="p-2 border-b border-[var(--border-color)] shrink-0 flex flex-col gap-1">
          <div class="text-[9px] font-bold text-[var(--dim-color)] tracking-wider">ENGINES</div>
          <div class="flex flex-wrap gap-1">
            <AppTooltip
              v-for="engine in catalog.engines"
              :key="engine.id"
              :text="`${engine.name} ${engine.version} (${engine.scope})${engine.is_installed ? '' : ` — not installed${engine.install_hint ? `: ${engine.install_hint}` : ''}`}`"
            >
              <span
                class="text-[9px] px-1 border"
                :class="engine.is_installed ? 'border-[var(--border-color)] text-[var(--fg-color)]' : 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]'"
              >{{ engine.id }}</span>
            </AppTooltip>
          </div>
          <div
            v-for="fault in catalog.faults"
            :key="fault.manifest_path"
            class="text-[9px] text-[var(--status-error-color)] break-all"
          >{{ fault.manifest_path }}: {{ fault.error }}</div>
        </div>

        <div class="px-2.5 py-1.5 border-b border-[var(--border-color)]/40 text-[9px] font-bold text-[var(--dim-color)] tracking-wider shrink-0">
          RUNS ({{ filteredRuns.length }})
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto p-1.5 flex flex-col gap-1">
          <div
            v-for="run in filteredRuns"
            :key="run.run_id"
            @click="selectRun(run)"
            class="p-2 border transition-colors cursor-pointer select-none"
            :class="selectedRunId === run.run_id
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)]'
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] bg-black/20'"
          >
            <div class="flex items-center justify-between gap-2 text-[10px] font-semibold mb-1">
              <span class="truncate">{{ run.target_id }}</span>
              <span class="text-[8px] px-1 border shrink-0" :class="statusClass(run)">
                {{ isRunning(run) ? 'RUNNING' : run.status.toUpperCase() }}{{ run.exit_code !== null && !isRunning(run) ? ` ${run.exit_code}` : '' }}
              </span>
            </div>
            <div class="flex items-center justify-between text-[9px] opacity-80">
              <span>{{ formatEpochMs(run.spawned_at_ms) }} • {{ formatDuration(run) }}</span>
              <span>{{ run.target_type }}{{ run.profiler_engine_id ? ` • ${run.profiler_engine_id}` : '' }}</span>
            </div>
          </div>

          <div v-if="!filteredRuns.length" class="p-6 text-center text-[var(--dim-color)] italic opacity-60">
            No runs yet. Every service and task run is profiled.
          </div>
        </div>
      </div>

      <div class="lg:col-span-8 flex flex-col min-h-0 h-full overflow-y-auto bg-[var(--bg-color)]">
        <template v-if="activeRun">
          <div class="h-8 px-3 border-b border-[var(--border-color)]/40 flex items-center justify-between gap-2 shrink-0 bg-[var(--ui-bg-card)]/20 text-[9px]">
            <span class="truncate text-[var(--dim-color)]">{{ activeRun.run_id }}</span>
            <div class="flex items-center gap-2 shrink-0">
              <AppTooltip v-if="activeRun.profile_artifact_path" :text="`Open ${activeRun.profile_artifact_path}`" position="bottom">
                <button
                  @click="openArtifact(activeRun)"
                  class="flex items-center gap-1 px-1.5 py-0.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] cursor-pointer"
                >
                  <ExternalLink class="w-3 h-3" />
                  <span>{{ activeRun.profiler_engine_id }}</span>
                </button>
              </AppTooltip>
              <Loader2 v-if="profilerStore.isLoadingProfile" class="w-3.5 h-3.5 animate-spin text-[var(--accent-color)]" />
            </div>
          </div>

          <div v-if="activeRun.failure_reason" class="px-3 py-1.5 text-[10px] text-[var(--status-error-color)] border-b border-[var(--border-color)]/40">
            {{ activeRun.failure_reason }}
          </div>

          <template v-if="activeProfile">
            <div class="grid grid-cols-3 sm:grid-cols-4 xl:grid-cols-6 gap-1.5 p-3 text-center text-[10px]">
              <div v-for="m in profileMetrics(activeProfile)" :key="m.label" class="border border-[var(--border-color)]/40 p-1.5 bg-black/30">
                <div class="text-[8px] text-[var(--dim-color)]">{{ m.label }}</div>
                <div class="font-bold text-[var(--fg-color)]">{{ m.value }}</div>
              </div>
            </div>

            <table class="mx-3 mb-3 text-left border-collapse text-[10px] border border-[var(--border-color)]">
              <thead class="bg-[var(--ui-bg-card)] border-b border-[var(--border-color)]">
                <tr>
                  <th class="p-2 font-semibold">PROCESS</th>
                  <th class="p-2 font-semibold text-right">AVG CPU</th>
                  <th class="p-2 font-semibold text-right">PEAK CPU</th>
                  <th class="p-2 font-semibold text-right">AVG RSS</th>
                  <th class="p-2 font-semibold text-right">PEAK RSS</th>
                  <th class="p-2 font-semibold text-right">INSTANCES</th>
                  <th class="p-2 font-semibold text-right">TICKS</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in activeProfile.processes" :key="p.process_name" class="border-b border-[var(--border-color)]/20">
                  <td class="p-2 truncate max-w-xs">{{ p.process_name }}</td>
                  <td class="p-2 text-right">{{ p.avg_cpu_percent.toFixed(1) }}%</td>
                  <td class="p-2 text-right">{{ p.peak_cpu_percent.toFixed(1) }}%</td>
                  <td class="p-2 text-right">{{ formatMbytes(p.avg_rss_memory_mbytes) }}</td>
                  <td class="p-2 text-right">{{ formatMbytes(p.peak_rss_memory_mbytes) }}</td>
                  <td class="p-2 text-right">{{ p.peak_instances_count }}</td>
                  <td class="p-2 text-right">{{ p.ticks_count }}</td>
                </tr>
              </tbody>
            </table>
          </template>
          <div v-else-if="activeProfile === null" class="flex-1 flex items-center justify-center p-6 text-center text-[var(--dim-color)] italic opacity-60">
            No samples for this run. A run shorter than one sampling interval has none.
          </div>
        </template>

        <div v-else class="flex-1 flex flex-col items-center justify-center p-6 text-center text-[var(--dim-color)] italic opacity-60">
          <Activity class="w-8 h-8 mb-2 opacity-40 text-[var(--accent-color)]" />
          <span>Select a run.</span>
        </div>
      </div>
    </div>

  </div>
</template>
