<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue';
import {
  Binary,
  Play,
  RotateCw,
  ZoomIn,
  ZoomOut,
  Maximize2,
  Code,
  Copy,
  Check,
  Download,
  Layers,
} from '@lucide/vue';
import { daemonCall } from '../../../utils/daemonClient';
import { AppTooltip, AppSelect } from '../../ui';
import type {
  LogicCaptureResult,
  DiscoveredInstrument,
} from '../../../types';

defineProps<{
  projectPath: string;
}>();

// Hardware discovery
const isScanning = ref<boolean>(false);
const connectedAnalyzer = ref<DiscoveredInstrument | null>(null);

// Capture parameters
const isCapturing = ref<boolean>(false);
const captureError = ref<string | null>(null);
const lastCapture = ref<LogicCaptureResult | null>(null);

const sampleRateHzStr = ref<string>('1000000'); // 1 MHz default
const durationMsStr = ref<string>('50'); // 50 ms default
const selectedDecoder = ref<string>('none');

// I2C pin mappings
const i2cSclPin = ref<string>('0');
const i2cSdaPin = ref<string>('1');

// SPI pin mappings
const spiClkPin = ref<string>('0');
const spiMosiPin = ref<string>('1');
const spiMisoPin = ref<string>('2');
const spiCsPin = ref<string>('3');

// UART pin mappings
const uartRxPin = ref<string>('0');
const uartBaud = ref<string>('115200');

// Channel customization
interface ChannelMeta {
  index: number;
  name: string;
  color: string;
  enabled: boolean;
}

const channelsMeta = ref<ChannelMeta[]>([
  { index: 0, name: 'D0 (SCL / CLK)', color: '#10b981', enabled: true },
  { index: 1, name: 'D1 (SDA / MOSI)', color: '#06b6d4', enabled: true },
  { index: 2, name: 'D2 (MISO)', color: '#f59e0b', enabled: true },
  { index: 3, name: 'D3 (CS)', color: '#ec4899', enabled: true },
  { index: 4, name: 'D4', color: '#8b5cf6', enabled: true },
  { index: 5, name: 'D5', color: '#14b8a6', enabled: true },
  { index: 6, name: 'D6', color: '#eab308', enabled: true },
  { index: 7, name: 'D7', color: '#6366f1', enabled: true },
]);

// Zoom and Pan
const zoomFactor = ref<number>(1.0);
const scrollOffsetX = ref<number>(0);
const hoverSampleIndex = ref<number | null>(null);

// WaveDrom modal
const showWaveDromModal = ref<boolean>(false);
const copiedWaveDrom = ref<boolean>(false);

// Canvas refs
const canvasRef = ref<HTMLCanvasElement | null>(null);
const containerRef = ref<HTMLDivElement | null>(null);
let resizeObserver: ResizeObserver | null = null;

// Select Options
const sampleRateOptions = [
  { label: '20 kHz', value: '20000' },
  { label: '50 kHz', value: '50000' },
  { label: '100 kHz', value: '100000' },
  { label: '200 kHz', value: '200000' },
  { label: '500 kHz', value: '500000' },
  { label: '1 MHz', value: '1000000' },
  { label: '2 MHz', value: '2000000' },
  { label: '4 MHz', value: '4000000' },
  { label: '8 MHz', value: '8000000' },
  { label: '12 MHz', value: '12000000' },
  { label: '16 MHz', value: '16000000' },
  { label: '24 MHz', value: '24000000' },
];

const durationOptions = [
  { label: '10 ms', value: '10' },
  { label: '20 ms', value: '20' },
  { label: '50 ms', value: '50' },
  { label: '100 ms', value: '100' },
  { label: '200 ms', value: '200' },
  { label: '500 ms', value: '500' },
  { label: '1000 ms (1s)', value: '1000' },
];

const decoderOptions = [
  { label: 'No Protocol Decoder', value: 'none' },
  { label: 'I2C Bus Decoder', value: 'i2c' },
  { label: 'SPI Bus Decoder', value: 'spi' },
  { label: 'UART Serial Decoder', value: 'uart' },
];

const pinOptions = [
  { label: 'D0', value: '0' },
  { label: 'D1', value: '1' },
  { label: 'D2', value: '2' },
  { label: 'D3', value: '3' },
  { label: 'D4', value: '4' },
  { label: 'D5', value: '5' },
  { label: 'D6', value: '6' },
  { label: 'D7', value: '7' },
];

const uartBaudOptions = [
  { label: '9600 baud', value: '9600' },
  { label: '19200 baud', value: '19200' },
  { label: '38400 baud', value: '38400' },
  { label: '57600 baud', value: '57600' },
  { label: '115200 baud', value: '115200' },
];

async function scanHardware() {
  isScanning.value = true;
  try {
    const list = await daemonCall('scan_connected_instruments', {});
    connectedAnalyzer.value = list.find((i: DiscoveredInstrument) => i.id === 'fx2lafw_8ch' || i.id === 'logic-analyzer-24mhz') ?? null;
  } catch (err: unknown) {
    console.error('Failed to scan logic analyzer hardware:', err);
  } finally {
    isScanning.value = false;
  }
}

async function performCapture() {
  if (isCapturing.value) return;
  isCapturing.value = true;
  captureError.value = null;

  let decoderArg: string | null = null;
  let decoderPinsArg: string | null = null;

  if (selectedDecoder.value === 'i2c') {
    decoderArg = 'i2c';
    decoderPinsArg = `scl=${i2cSclPin.value},sda=${i2cSdaPin.value}`;
  } else if (selectedDecoder.value === 'spi') {
    decoderArg = 'spi';
    decoderPinsArg = `clk=${spiClkPin.value},mosi=${spiMosiPin.value},miso=${spiMisoPin.value},cs=${spiCsPin.value}`;
  } else if (selectedDecoder.value === 'uart') {
    decoderArg = 'uart';
    decoderPinsArg = `rx=${uartRxPin.value},baudrate=${uartBaud.value}`;
  }

  try {
    const res = await daemonCall('capture_logic_analyzer', {
      sampleRateHz: BigInt(sampleRateHzStr.value),
      durationMs: BigInt(durationMsStr.value),
      decoder: decoderArg,
      decoderPins: decoderPinsArg,
    });
    lastCapture.value = res;
    scrollOffsetX.value = 0;
    drawTraces();
  } catch (err: unknown) {
    captureError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isCapturing.value = false;
  }
}

function handleZoomIn() {
  zoomFactor.value = Math.min(50.0, zoomFactor.value * 1.5);
  drawTraces();
}

function handleZoomOut() {
  zoomFactor.value = Math.max(0.2, zoomFactor.value / 1.5);
  drawTraces();
}

function handleResetZoom() {
  zoomFactor.value = 1.0;
  scrollOffsetX.value = 0;
  drawTraces();
}

function copyWaveDrom() {
  if (!lastCapture.value?.wavedromJson) return;
  navigator.clipboard.writeText(lastCapture.value.wavedromJson);
  copiedWaveDrom.value = true;
  setTimeout(() => {
    copiedWaveDrom.value = false;
  }, 2000);
}

function exportCsv() {
  if (!lastCapture.value || !lastCapture.value.channels) return;
  const channels = lastCapture.value.channels;
  const samplesCount = lastCapture.value.samplesCount;
  const sampleRate = Number(lastCapture.value.sampleRateHz);

  let csv = 'sample,time_us,d0,d1,d2,d3,d4,d5,d6,d7\n';
  for (let s = 0; s < samplesCount; s++) {
    const timeUs = ((s / sampleRate) * 1000000).toFixed(3);
    const pinVals = channels.map((ch) => ch[s] ?? 0).join(',');
    csv += `${s},${timeUs},${pinVals}\n`;
  }

  const blob = new Blob([csv], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `logic_capture_${Date.now()}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

// Canvas Drawing Engine for 8 Digital Tracks
function drawTraces() {
  const canvas = canvasRef.value;
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;
  if (width === 0 || height === 0) return;

  // Clear background
  ctx.fillStyle = '#0a0d14';
  ctx.fillRect(0, 0, width, height);

  const topRulerHeight = 28;
  const leftLabelWidth = 140;
  const waveformAreaWidth = width - leftLabelWidth;
  const waveformAreaHeight = height - topRulerHeight;

  const numTracks = 8;
  const trackHeight = waveformAreaHeight / numTracks;

  // Draw Top Time Ruler Background
  ctx.fillStyle = '#0f1420';
  ctx.fillRect(leftLabelWidth, 0, waveformAreaWidth, topRulerHeight);
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.1)';
  ctx.lineWidth = 1;
  ctx.strokeRect(leftLabelWidth, 0, waveformAreaWidth, topRulerHeight);

  // Draw Left Label Column Background
  ctx.fillStyle = '#0c101a';
  ctx.fillRect(0, 0, leftLabelWidth, height);
  ctx.strokeRect(0, 0, leftLabelWidth, height);

  if (!lastCapture.value || !lastCapture.value.channels || lastCapture.value.channels.length === 0) {
    ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
    ctx.font = '12px monospace';
    ctx.fillText('No capture data. Click "Capture" to acquire digital stream.', leftLabelWidth + 30, height / 2);
    return;
  }

  const capture = lastCapture.value;
  const samplesCount = capture.samplesCount;
  const sampleRate = Number(capture.sampleRateHz);
  const totalDurationUs = (samplesCount / sampleRate) * 1000000;

  // Calculate scaling
  const visibleSamples = Math.max(10, Math.floor(samplesCount / zoomFactor.value));
  const pixelsPerSample = waveformAreaWidth / visibleSamples;

  // Draw time ruler ticks
  const tickCount = 10;
  const timeStepUs = totalDurationUs / (tickCount * zoomFactor.value);

  ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
  ctx.font = '9px monospace';

  for (let t = 0; t <= tickCount; t++) {
    const x = leftLabelWidth + (t * waveformAreaWidth) / tickCount;
    const timeValUs = (scrollOffsetX.value * (1000000 / sampleRate) + t * timeStepUs).toFixed(1);

    ctx.beginPath();
    ctx.moveTo(x, topRulerHeight - 6);
    ctx.lineTo(x, topRulerHeight);
    ctx.stroke();

    ctx.fillText(`${timeValUs}µs`, x + 3, topRulerHeight - 10);
  }

  // Draw Channels
  for (let ch = 0; ch < numTracks; ch++) {
    const trackTop = topRulerHeight + ch * trackHeight;
    const meta = channelsMeta.value[ch] ?? { name: `D${ch}`, color: '#10b981', enabled: true };
    const samples = capture.channels[ch] ?? [];

    // Track divider line
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.06)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, trackTop + trackHeight);
    ctx.lineTo(width, trackTop + trackHeight);
    ctx.stroke();

    // Channel Left Label
    ctx.fillStyle = meta.color;
    ctx.font = 'bold 11px monospace';
    ctx.fillText(meta.name, 12, trackTop + trackHeight / 2 + 4);

    // Current State Indicator on label
    const curVal = hoverSampleIndex.value !== null && hoverSampleIndex.value < samples.length
      ? samples[hoverSampleIndex.value]
      : samples[samples.length - 1] ?? 0;

    ctx.fillStyle = curVal === 1 ? '#10b981' : '#6b7280';
    ctx.fillRect(leftLabelWidth - 24, trackTop + trackHeight / 2 - 6, 14, 12);
    ctx.fillStyle = '#000000';
    ctx.font = 'bold 9px monospace';
    ctx.fillText(String(curVal), leftLabelWidth - 20, trackTop + trackHeight / 2 + 3);

    // Waveform Trace
    const highY = trackTop + trackHeight * 0.2;
    const lowY = trackTop + trackHeight * 0.8;

    ctx.save();
    ctx.beginPath();
    ctx.rect(leftLabelWidth, trackTop, waveformAreaWidth, trackHeight);
    ctx.clip();

    ctx.strokeStyle = meta.color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();

    let started = false;
    let prevVal = -1;

    for (let s = 0; s < visibleSamples; s++) {
      const sampleIdx = Math.floor(scrollOffsetX.value + s);
      if (sampleIdx >= samples.length) break;

      const val = samples[sampleIdx];
      const x = leftLabelWidth + s * pixelsPerSample;
      const y = val === 1 ? highY : lowY;

      if (!started) {
        ctx.moveTo(x, y);
        started = true;
      } else {
        if (val !== prevVal) {
          // Square transition edge
          ctx.lineTo(x, prevVal === 1 ? highY : lowY);
          ctx.lineTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }
      prevVal = val;
    }

    ctx.stroke();
    ctx.restore();
  }

  // Draw Hover Vertical Cursor Line
  if (hoverSampleIndex.value !== null && hoverSampleIndex.value >= scrollOffsetX.value) {
    const relSample = hoverSampleIndex.value - scrollOffsetX.value;
    const cursorX = leftLabelWidth + relSample * pixelsPerSample;

    if (cursorX >= leftLabelWidth && cursorX <= width) {
      ctx.strokeStyle = 'rgba(245, 158, 11, 0.7)';
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(cursorX, topRulerHeight);
      ctx.lineTo(cursorX, height);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }
}

function handleMouseMove(e: MouseEvent) {
  if (!lastCapture.value || !containerRef.value) return;
  const rect = containerRef.value.getBoundingClientRect();
  const leftLabelWidth = 140;
  const mouseX = e.clientX - rect.left;

  if (mouseX < leftLabelWidth) {
    hoverSampleIndex.value = null;
    drawTraces();
    return;
  }

  const waveformAreaWidth = rect.width - leftLabelWidth;
  const samplesCount = lastCapture.value.samplesCount;
  const visibleSamples = Math.max(10, Math.floor(samplesCount / zoomFactor.value));
  const pixelsPerSample = waveformAreaWidth / visibleSamples;

  const relX = mouseX - leftLabelWidth;
  const sampleIdx = Math.floor(scrollOffsetX.value + relX / pixelsPerSample);

  if (sampleIdx >= 0 && sampleIdx < samplesCount) {
    hoverSampleIndex.value = sampleIdx;
  } else {
    hoverSampleIndex.value = null;
  }
  drawTraces();
}

function handleWheel(e: WheelEvent) {
  if (!lastCapture.value) return;
  e.preventDefault();
  if (e.deltaY < 0) {
    handleZoomIn();
  } else {
    handleZoomOut();
  }
}

function resizeCanvas() {
  const canvas = canvasRef.value;
  const container = containerRef.value;
  if (!canvas || !container) return;

  const rect = container.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.floor(rect.width);
  const h = Math.floor(rect.height);

  if (w > 0 && h > 0) {
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;

    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.scale(dpr, dpr);
    }
    drawTraces();
  }
}

onMounted(() => {
  scanHardware();
  if (containerRef.value) {
    resizeObserver = new ResizeObserver(() => {
      resizeCanvas();
    });
    resizeObserver.observe(containerRef.value);
  }
  // Initial dummy capture for visual feedback
  performCapture();
});

onUnmounted(() => {
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});

watch([selectedDecoder, i2cSclPin, i2cSdaPin, spiClkPin, spiMosiPin, spiMisoPin, spiCsPin, uartRxPin, uartBaud], () => {
  if (lastCapture.value) {
    performCapture();
  }
});
</script>

<template>
  <div class="h-full flex flex-col min-w-0 bg-[var(--bg-color)] text-[var(--fg-color)] overflow-hidden">
    <!-- Logic Analyzer Top Toolbar -->
    <div class="h-9 ui-surface-title border-b border-[var(--border-color)] px-3 flex items-center justify-between gap-2 shrink-0 overflow-x-auto">
      <div class="flex items-center gap-2 shrink-0">
        <Binary class="w-3.5 h-3.5 text-[var(--status-ok-color)] shrink-0" />
        <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)]">24MHz 8-Ch Logic Analyzer (FX2LP)</span>

        <!-- Hardware Status Badge -->
        <span
          class="text-[9px] font-mono px-1.5 py-0.5 border"
          :class="connectedAnalyzer?.isConnected
            ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold'
            : 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)]'"
        >
          {{ connectedAnalyzer?.isConnected ? 'FX2LP Connected' : 'Emulated / Offline' }}
        </span>

        <AppTooltip text="Scan for USB FX2LP logic analyzer hardware">
          <button
            @click="scanHardware"
            :disabled="isScanning"
            class="ui-icon-btn !h-6 !w-6 disabled:opacity-50"
          >
            <RotateCw class="w-3 h-3" :class="{ 'animate-spin': isScanning }" />
          </button>
        </AppTooltip>
      </div>

      <!-- Action Toolbar Controls -->
      <div class="flex items-center gap-1.5 shrink-0">
        <!-- Capture Trigger -->
        <button
          @click="performCapture"
          :disabled="isCapturing"
          class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] hover:opacity-90 transition-all flex items-center gap-1 cursor-pointer disabled:opacity-50"
        >
          <Play class="w-3 h-3" />
          <span>{{ isCapturing ? 'Acquiring...' : 'Capture' }}</span>
        </button>

        <div class="h-4 w-px bg-[var(--border-color)] mx-0.5" />

        <!-- Zoom Controls -->
        <div class="flex items-center border border-[var(--border-color)]">
          <AppTooltip text="Zoom In (Scroll Up)">
            <button
              @click="handleZoomIn"
              class="ui-icon-btn !h-6 !w-6 !border-0"
            >
              <ZoomIn class="w-3 h-3" />
            </button>
          </AppTooltip>
          <AppTooltip text="Zoom Out (Scroll Down)">
            <button
              @click="handleZoomOut"
              class="ui-icon-btn !h-6 !w-6 !border-0 !border-l !border-[var(--border-color)]"
            >
              <ZoomOut class="w-3 h-3" />
            </button>
          </AppTooltip>
          <AppTooltip text="Reset Zoom & Fit">
            <button
              @click="handleResetZoom"
              class="ui-icon-btn !h-6 !w-6 !border-0 !border-l !border-[var(--border-color)]"
            >
              <Maximize2 class="w-3 h-3" />
            </button>
          </AppTooltip>
        </div>

        <!-- WaveDrom JSON export -->
        <AppTooltip text="View WaveDrom timing diagram JSON representation">
          <button
            @click="showWaveDromModal = !showWaveDromModal"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors flex items-center gap-1 cursor-pointer"
          >
            <Layers class="w-3 h-3" />
            <span>WaveDrom</span>
          </button>
        </AppTooltip>

        <!-- CSV Export -->
        <AppTooltip text="Export raw digital acquisition to CSV">
          <button
            @click="exportCsv"
            :disabled="!lastCapture"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors flex items-center gap-1 cursor-pointer disabled:opacity-40"
          >
            <Download class="w-3 h-3" />
            <span>CSV</span>
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Parameter Config Bar -->
    <div class="h-9 border-b border-[var(--border-color)] px-3 bg-[var(--ui-bg-card)]/50 flex items-center justify-between gap-2 text-xs shrink-0 overflow-x-auto">
      <div class="flex items-center gap-2.5 shrink-0">
        <div class="flex items-center gap-1.5">
          <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Rate:</span>
          <AppSelect v-model="sampleRateHzStr" :options="sampleRateOptions" class="w-28 text-xs" />
        </div>

        <div class="flex items-center gap-1.5">
          <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Duration:</span>
          <AppSelect v-model="durationMsStr" :options="durationOptions" class="w-28 text-xs" />
        </div>

        <div class="flex items-center gap-1.5">
          <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Decoder:</span>
          <AppSelect v-model="selectedDecoder" :options="decoderOptions" class="w-44 text-xs" />
        </div>
      </div>

      <!-- Protocol Decoder Pin Mappings -->
      <div v-if="selectedDecoder === 'i2c'" class="flex items-center gap-2 font-mono text-[10px] bg-[var(--surface-bg)] px-2 py-0.5 border border-[var(--status-ok-border)] shrink-0">
        <span class="text-[var(--status-ok-color)] font-bold uppercase text-[9px]">I2C Pins:</span>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">SCL:</span>
          <AppSelect v-model="i2cSclPin" :options="pinOptions" class="w-16" />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">SDA:</span>
          <AppSelect v-model="i2cSdaPin" :options="pinOptions" class="w-16" />
        </div>
      </div>

      <div v-else-if="selectedDecoder === 'spi'" class="flex items-center gap-2 font-mono text-[10px] bg-[var(--surface-bg)] px-2 py-0.5 border border-[var(--accent-color)]/40 shrink-0">
        <span class="text-[var(--accent-color)] font-bold uppercase text-[9px]">SPI Pins:</span>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">CLK:</span>
          <AppSelect v-model="spiClkPin" :options="pinOptions" class="w-16" />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">MOSI:</span>
          <AppSelect v-model="spiMosiPin" :options="pinOptions" class="w-16" />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">MISO:</span>
          <AppSelect v-model="spiMisoPin" :options="pinOptions" class="w-16" />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">CS:</span>
          <AppSelect v-model="spiCsPin" :options="pinOptions" class="w-16" />
        </div>
      </div>

      <div v-else-if="selectedDecoder === 'uart'" class="flex items-center gap-2 font-mono text-[10px] bg-[var(--surface-bg)] px-2 py-0.5 border border-[var(--status-warn-border)] shrink-0">
        <span class="text-[var(--status-warn-color)] font-bold uppercase text-[9px]">UART Pins:</span>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">RX:</span>
          <AppSelect v-model="uartRxPin" :options="pinOptions" class="w-16" />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-[var(--dim-color)]">Baud:</span>
          <AppSelect v-model="uartBaud" :options="uartBaudOptions" class="w-28" />
        </div>
      </div>
    </div>

    <!-- Main Logic Visualizer & Packet Table -->
    <div class="flex-1 flex flex-col min-h-0 min-w-0">
      <!-- 8-Channel Digital Trace Canvas -->
      <div
        ref="containerRef"
        @mousemove="handleMouseMove"
        @wheel="handleWheel"
        class="flex-1 min-h-[300px] bg-[#0a0d14] relative overflow-hidden select-none cursor-crosshair"
      >
        <canvas ref="canvasRef" class="w-full h-full block" />

        <!-- Hover inspection tooltip -->
        <div
          v-if="hoverSampleIndex !== null && lastCapture"
          class="absolute top-8 left-38 bg-[var(--surface-bg)]/95 border border-[var(--border-color)] p-2 text-[10px] font-mono text-[var(--fg-color)] pointer-events-none flex flex-col gap-1 shadow-lg"
        >
          <div class="flex justify-between gap-4 border-b border-[var(--border-color)] pb-1">
            <span class="text-[var(--status-warn-color)] font-bold">Sample #{{ hoverSampleIndex }}</span>
            <span class="text-[var(--dim-color)]">
              {{ ((hoverSampleIndex / Number(lastCapture.sampleRateHz)) * 1000000).toFixed(2) }}µs
            </span>
          </div>
          <div class="grid grid-cols-4 gap-x-2 gap-y-0.5">
            <span
              v-for="ch in 8"
              :key="ch - 1"
              :style="{ color: channelsMeta[ch - 1].color }"
            >
              D{{ ch - 1 }}:
              <strong class="text-[var(--fg-color)]">{{ lastCapture.channels[ch - 1]?.[hoverSampleIndex] ?? 0 }}</strong>
            </span>
          </div>
        </div>
      </div>

      <!-- Decoded Protocol Packets / Frames Stream -->
      <div class="h-44 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col min-h-0 shrink-0">
        <div class="border-b border-[var(--border-color)] px-3 py-1.5 bg-[var(--ui-bg-card)]/40 flex items-center justify-between text-xs">
          <span class="font-bold uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-1.5">
            <Code class="w-3.5 h-3.5 text-[var(--status-ok-color)]" />
            <span>Decoded Protocol Stream ({{ lastCapture?.decodedPackets?.length ?? 0 }} Packets)</span>
          </span>
          <span class="text-[10px] font-mono text-[var(--dim-color)]">
            Decoder: {{ selectedDecoder.toUpperCase() }}
          </span>
        </div>

        <div class="flex-1 overflow-y-auto p-2 font-mono text-xs select-text">
          <table class="w-full text-left border-collapse">
            <thead>
              <tr class="text-[10px] uppercase text-[var(--dim-color)] border-b border-[var(--border-color)]">
                <th class="py-1 px-2">Sample</th>
                <th class="py-1 px-2">Time (µs)</th>
                <th class="py-1 px-2">Protocol</th>
                <th class="py-1 px-2">Summary</th>
                <th class="py-1 px-2">Payload Hex</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(pkt, idx) in lastCapture?.decodedPackets ?? []"
                :key="`${pkt.startSampleIndex}-${pkt.proto}-${idx}`"
                class="border-b border-[var(--border-color)]/40 hover:bg-[var(--surface-bg)] text-[11px]"
              >
                <td class="py-1 px-2 text-[var(--dim-color)]">#{{ pkt.startSampleIndex }}</td>
                <td class="py-1 px-2 text-[var(--fg-color)]">
                  {{
                    lastCapture?.sampleRateHz
                      ? ((Number(pkt.startSampleIndex) / Number(lastCapture.sampleRateHz)) * 1000000).toFixed(1)
                      : '---'
                  }}µs
                </td>
                <td class="py-1 px-2">
                  <span class="px-1 py-0.2 text-[9px] border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold uppercase">
                    {{ pkt.proto }}
                  </span>
                </td>
                <td class="py-1 px-2 text-[var(--fg-color)]">{{ pkt.summary }}</td>
                <td class="py-1 px-2 text-[var(--status-warn-color)] font-bold">{{ pkt.dataHex || '---' }}</td>
              </tr>

              <tr v-if="!lastCapture?.decodedPackets || lastCapture.decodedPackets.length === 0">
                <td colspan="5" class="py-6 text-center text-[var(--dim-color)] italic">
                  No protocol packets decoded in the current capture window.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- WaveDrom Modal (Collapsible overlay) -->
    <div
      v-if="showWaveDromModal"
      class="border-t border-[var(--border-color)] bg-[var(--surface-bg)] p-3 max-h-60 overflow-y-auto font-mono text-xs text-[var(--fg-color)] flex flex-col gap-2 shrink-0"
    >
      <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-1">
        <span class="font-bold text-[var(--accent-color)] uppercase flex items-center gap-1.5">
          <Layers class="w-3.5 h-3.5" />
          <span>WaveDrom Timing JSON Representation</span>
        </span>
        <div class="flex items-center gap-2">
          <button
            @click="copyWaveDrom"
            class="h-6 px-2 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] text-[var(--fg-color)] flex items-center gap-1 cursor-pointer transition-colors"
          >
            <Check v-if="copiedWaveDrom" class="w-3 h-3 text-[var(--status-ok-color)]" />
            <Copy v-else class="w-3 h-3" />
            <span>{{ copiedWaveDrom ? 'Copied' : 'Copy JSON' }}</span>
          </button>
          <button
            @click="showWaveDromModal = false"
            class="ui-icon-btn !h-6 !w-6"
          >
            ✕
          </button>
        </div>
      </div>

      <pre class="whitespace-pre text-[11px] leading-tight text-[var(--fg-color)]">{{
        lastCapture?.wavedromJson || '{\n  "signal": []\n}'
      }}</pre>
    </div>
  </div>
</template>
