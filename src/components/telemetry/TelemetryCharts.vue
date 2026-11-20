<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import AppTooltip from '../ui/AppTooltip.vue';
import type { SystemTelemetryData } from '../../stores/coreStore';

const props = defineProps<{
  metrics: SystemTelemetryData;
}>();

// ── Network Sparkline History Buffer (20 ticks) ──────────────────────────
interface NetPoint {
  rx: number;
  tx: number;
}

const netHistory = ref<NetPoint[]>(
  Array.from({ length: 20 }, () => ({ rx: 0, tx: 0 }))
);

watch(
  () => [props.metrics.net_rx_kbytes_per_sec, props.metrics.net_tx_kbytes_per_sec],
  ([rx, tx]) => {
    const nextRx = Number(rx) || 0;
    const nextTx = Number(tx) || 0;
    netHistory.value.push({ rx: nextRx, tx: nextTx });
    if (netHistory.value.length > 20) {
      netHistory.value.shift();
    }
  },
  { immediate: true }
);

const sparklineSvgData = computed(() => {
  const points = netHistory.value;
  const width = 52;
  const height = 18;
  const count = points.length;
  if (count < 2) return { rxPoints: '', txPoints: '', maxSpeedLabel: '0' };

  let maxVal = 50; // Minimum baseline 50 KB/s
  for (const p of points) {
    if (p.rx > maxVal) maxVal = p.rx;
    if (p.tx > maxVal) maxVal = p.tx;
  }

  const stepX = width / (count - 1);
  const rxCoords = points.map((p, idx) => {
    const x = (idx * stepX).toFixed(1);
    const y = (height - (p.rx / maxVal) * (height - 2) - 1).toFixed(1);
    return `${x},${y}`;
  });

  const txCoords = points.map((p, idx) => {
    const x = (idx * stepX).toFixed(1);
    const y = (height - (p.tx / maxVal) * (height - 2) - 1).toFixed(1);
    return `${x},${y}`;
  });

  const maxLabel = maxVal >= 1024 ? `${(maxVal / 1024).toFixed(1)}M` : `${maxVal.toFixed(0)}K`;

  return {
    rxPoints: rxCoords.join(' '),
    txPoints: txCoords.join(' '),
    maxSpeedLabel: maxLabel,
  };
});

// ── Fixed-Width Network Speed Readout ──────────────────────────────────
function formatSpeed(kbs?: number): string {
  const v = Number(kbs) || 0;
  if (v >= 1024 * 100) return `${(v / 1024).toFixed(0)}M`;
  if (v >= 1024) return `${(v / 1024).toFixed(1)}M`;
  return `${v.toFixed(0)}K`;
}

const formattedNetSpeed = computed(() => {
  const rx = formatSpeed(props.metrics.net_rx_kbytes_per_sec);
  const tx = formatSpeed(props.metrics.net_tx_kbytes_per_sec);
  return `↓${rx} ↑${tx}`;
});

// ── Tooltip Formatter Strings ──────────────────────────────────────────
const cpuTooltipText = computed(() => {
  const cores = props.metrics.cpu_cores || [];
  const overall = props.metrics.cpu || `${props.metrics.cpu_usage_percent?.toFixed(1) ?? 0}%`;
  const coreCount = cores.length || 1;
  return `CPU: ${overall} (${coreCount} cores)`;
});

const ramTooltipText = computed(() => {
  const ramStr = props.metrics.ram || '-- / --';
  const ramPct = props.metrics.ram_percent?.toFixed(0) || '0';
  const swpStr = props.metrics.swap || '-- / --';
  const swpPct = props.metrics.swap_percent?.toFixed(0) || '0';
  return `RAM: ${ramStr} (${ramPct}%) | SWAP: ${swpStr} (${swpPct}%)`;
});

const diskTooltipText = computed(() => {
  const diskStr = props.metrics.disk || '-- / --';
  const diskPct = props.metrics.disk_percent?.toFixed(0) || '0';
  return `Disk (/): ${diskStr} (${diskPct}%)`;
});

const netTooltipText = computed(() => {
  return `Network: ${props.metrics.net || '↓ 0 KB/s ↑ 0 KB/s'}`;
});

function getBarColor(pct: number) {
  if (pct >= 90) return 'bg-[var(--status-error-color)]';
  if (pct >= 75) return 'bg-[var(--status-warn-color)]';
  return 'bg-[var(--accent-color)]';
}
</script>

<template>
  <div class="flex items-center gap-1.5 sm:gap-2 text-[9px] font-mono select-none">
    
    <!-- ── 1. CPU MULTI-CORE EQUALIZER ─────────────────────────────── -->
    <AppTooltip :text="cpuTooltipText" position="bottom">
      <div class="h-7 flex items-center gap-1.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)] hover:border-[var(--accent-color)] px-1.5 py-0.5 transition-colors cursor-default">
        <span class="text-[8px] font-bold text-[var(--dim-color)] tracking-tighter">CPU</span>
        
        <!-- Multi-Core Bars Array -->
        <div class="flex items-end gap-[1.5px] h-4.5">
          <template v-if="metrics.cpu_cores && metrics.cpu_cores.length > 0">
            <div 
              v-for="(corePct, coreNumber) in metrics.cpu_cores" 
              :key="`core-${coreNumber}`"
              class="w-[3px] sm:w-[4px] h-full bg-[var(--border-color)]/40 flex flex-col justify-end overflow-hidden"
            >
              <div 
                class="w-full transition-all duration-300"
                :class="getBarColor(corePct)"
                :style="{ height: `${Math.min(100, Math.max(8, corePct))}%` }"
              />
            </div>
          </template>
          <!-- Fallback single bar if cores not populated -->
          <template v-else>
            <div class="w-8 h-full bg-[var(--border-color)]/40 flex flex-col justify-end overflow-hidden">
              <div 
                class="w-full transition-all duration-300 bg-[var(--accent-color)]"
                :style="{ height: `${Math.min(100, Math.max(8, metrics.cpu_usage_percent || 0))}%` }"
              />
            </div>
          </template>
        </div>
      </div>
    </AppTooltip>

    <!-- ── 2. RAM & SWAP COMBINED GAUGE ────────────────────────────── -->
    <AppTooltip :text="ramTooltipText" position="bottom">
      <div class="h-7 flex items-center gap-2 bg-[var(--ui-bg-card)] border border-[var(--border-color)] hover:border-[var(--accent-color)] px-1.5 py-0.5 transition-colors cursor-default">
        <!-- RAM Bar -->
        <div class="flex items-center gap-1">
          <span class="text-[8px] font-bold text-[var(--dim-color)]">RAM</span>
          <div class="w-2.5 sm:w-3 h-4.5 bg-[var(--border-color)]/40 flex flex-col justify-end overflow-hidden">
            <div 
              class="w-full transition-all duration-300"
              :class="getBarColor(metrics.ram_percent || 0)"
              :style="{ height: `${Math.min(100, Math.max(8, metrics.ram_percent || 0))}%` }"
            />
          </div>
        </div>

        <!-- SWAP Bar -->
        <div class="flex items-center gap-1">
          <span class="text-[8px] font-bold text-[var(--dim-color)]">SWP</span>
          <div class="w-2.5 sm:w-3 h-4.5 bg-[var(--border-color)]/40 flex flex-col justify-end overflow-hidden">
            <div 
              class="w-full transition-all duration-300 bg-[var(--status-warn-color)]"
              :style="{ height: `${Math.min(100, Math.max(8, metrics.swap_percent || 0))}%` }"
            />
          </div>
        </div>
      </div>
    </AppTooltip>

    <!-- ── 3. DISK GAUGE (Auto-collapses in split/compact view) ───── -->
    <AppTooltip :text="diskTooltipText" position="bottom">
      <div class="hidden lg:flex h-7 items-center gap-1.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)] hover:border-[var(--accent-color)] px-1.5 py-0.5 transition-colors cursor-default">
        <span class="text-[8px] font-bold text-[var(--dim-color)]">DSK</span>
        <div class="w-2.5 sm:w-3 h-4.5 bg-[var(--border-color)]/40 flex flex-col justify-end overflow-hidden">
          <div 
            class="w-full transition-all duration-300 bg-[var(--status-info-color)]"
            :style="{ height: `${Math.min(100, Math.max(8, metrics.disk_percent || 0))}%` }"
          />
        </div>
      </div>
    </AppTooltip>

    <!-- ── 4. NETWORK DUAL LIVE SPARKLINE + FIXED READOUT ─────────── -->
    <AppTooltip :text="netTooltipText" position="bottom">
      <div class="h-7 flex items-center gap-1.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)] hover:border-[var(--accent-color)] px-1.5 py-0.5 transition-colors cursor-default">
        <span class="text-[8px] font-bold text-[var(--dim-color)]">NET</span>
        <svg 
          class="w-11 sm:w-12 h-4.5 overflow-visible" 
          viewBox="0 0 52 18"
          preserveAspectRatio="none"
        >
          <!-- Grid baseline -->
          <line x1="0" y1="17" x2="52" y2="17" stroke="rgba(255,255,255,0.06)" stroke-width="1" />
          
          <!-- Upload Polyline (dim dashed) -->
          <polyline
            v-if="sparklineSvgData.txPoints"
            :points="sparklineSvgData.txPoints"
            fill="none"
            stroke="rgba(255, 255, 255, 0.35)"
            stroke-width="1"
            stroke-dasharray="2 1"
          />

          <!-- Download Polyline (accent solid) -->
          <polyline
            v-if="sparklineSvgData.rxPoints"
            :points="sparklineSvgData.rxPoints"
            fill="none"
            stroke="var(--accent-color)"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>

        <!-- Fixed-width tabular numeric speed slot -->
        <span class="w-[66px] tabular-nums font-mono text-right text-[9px] font-bold text-[var(--fg-color)] tracking-tighter shrink-0 select-none">
          {{ formattedNetSpeed }}
        </span>
      </div>
    </AppTooltip>

  </div>
</template>
