<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { storeToRefs } from 'pinia';
import {
  Activity,
  Play,
  Square,
  Zap,
  RotateCw,
  Copy,
  Check,
  Code,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  X,
  Loader2,
  Sparkles,
  Rows2,
  LayoutGrid,
  Download,
  BarChart2,
  Sigma,
} from '@lucide/vue';
import { daemonCall } from '../../../utils/daemonClient';
import { onDaemonEvent } from '../../../utils/daemonEvents';
import { AppTooltip, AppSelect, AppToggle, AppInput } from '../../ui';
import { useElectronicsStore } from '../../../stores/electronicsStore';
import { usePressAndHold } from '../../../composables/usePressAndHold';
import {
  computeMathTrace,
  computeFftSpectrum,
  computeExtendedMetrics,
  exportCaptureCsv,
  exportCaptureJson,
} from '../../../utils/electronicsDsp';
import type {
  OscilloscopeCalibrationResult,
} from '../../../types';

const props = defineProps<{
  projectPath: string;
  projectId: number;
}>();

const store = useElectronicsStore();
const {
  isScanning,
  connectedOwon,
  supportedOscilloscopes,
  selectedScopeId,
  activeScopeDescriptor,
  isCapturing,
  isContinuousRunning,
  isAutoSetting,
  lastCapture,
  captureError,
  ch1Enabled,
  ch1VoltsPerDiv,
  ch1OffsetMv,
  ch1Coupling,
  ch1ProbeAtten,
  ch2Enabled,
  ch2VoltsPerDiv,
  ch2OffsetMv,
  ch2Coupling,
  ch2ProbeAtten,
  timebaseDivUs,
  triggerSource,
  triggerSlope,
  triggerSweep,
  triggerLevelMv,
  showTimeCursors,
  showVoltCursors,
  cursorTime1Us,
  cursorTime2Us,
  cursorVolt1Mv,
  cursorVolt2Mv,
  displayMode,
  lastCalSummary,
  mathEnabled,
  mathOp,
  showFft,
  showExtendedMetrics,
} = storeToRefs(store);

// Press-and-hold accelerating triggers
const { createHoldTrigger } = usePressAndHold();

// CH1 Steppers
const holdCh1VDivDec = createHoldTrigger(() => store.stepVoltsPerDiv(1, -1));
const holdCh1VDivInc = createHoldTrigger(() => store.stepVoltsPerDiv(1, 1));
const holdCh1OffsetDec = createHoldTrigger(() => { store.ch1OffsetMv -= 100; });
const holdCh1OffsetInc = createHoldTrigger(() => { store.ch1OffsetMv += 100; });

// CH2 Steppers
const holdCh2VDivDec = createHoldTrigger(() => store.stepVoltsPerDiv(2, -1));
const holdCh2VDivInc = createHoldTrigger(() => store.stepVoltsPerDiv(2, 1));
const holdCh2OffsetDec = createHoldTrigger(() => { store.ch2OffsetMv -= 100; });
const holdCh2OffsetInc = createHoldTrigger(() => { store.ch2OffsetMv += 100; });

// Trigger Level Steppers
const holdTriggerLevelDec = createHoldTrigger(() => { store.triggerLevelMv -= 50; });
const holdTriggerLevelInc = createHoldTrigger(() => { store.triggerLevelMv += 50; });

// Timebase Steppers
const holdTimebaseDec = createHoldTrigger(() => store.stepTimebase(-1));
const holdTimebaseInc = createHoldTrigger(() => store.stepTimebase(1));

// UI-only state
const showAgentDigest = ref<boolean>(false);
const copiedDigest = ref<boolean>(false);

// Calibration modal state
const showCalibrationModal = ref<boolean>(false);
const activeCalTab = ref<'probe' | 'offset'>('probe');
const isCalibrating = ref<boolean>(false);
const calChannel = ref<string>('CH1');
const calProbeAtten = ref<string>('1');
const calResult = ref<OscilloscopeCalibrationResult | null>(null);
const calError = ref<string | null>(null);

// Dynamic Scope Model Selector derived from active hardware descriptor
const scopeModels = computed(() => {
  if (!supportedOscilloscopes.value || supportedOscilloscopes.value.length === 0) {
    return [{ label: 'Owon VDS1022 (2CH, 25MHz)', value: 'owon_vds1022' }];
  }
  return supportedOscilloscopes.value.map((desc) => ({
    label: `${desc.name} (${desc.channelCount}CH, ${Math.round(Number(desc.bandwidthHz) / 1e6)}MHz)`,
    value: desc.id,
  }));
});

// Canvas refs
const canvasRef = ref<HTMLCanvasElement | null>(null);
const fftCanvasRef = ref<HTMLCanvasElement | null>(null);
const containerRef = ref<HTMLDivElement | null>(null);
let resizeObserver: ResizeObserver | null = null;

// Cursor Drag & Drop State
type DragCursorType = 'T1' | 'T2' | 'V1' | 'V2' | null;
const activeDragCursor = ref<DragCursorType>(null);
const hoveredCursor = ref<DragCursorType>(null);

const canvasCursorStyle = computed(() => {
  const target = activeDragCursor.value || hoveredCursor.value;
  if (target === 'T1' || target === 'T2') return 'ew-resize';
  if (target === 'V1' || target === 'V2') return 'ns-resize';
  return 'crosshair';
});

// Phase 2 Lab Tools Computeds
const fftResult = computed(() => {
  if (!lastCapture.value) return null;
  const cap = lastCapture.value;
  const samples = cap.ch1Samples && cap.ch1Samples.length > 0 ? cap.ch1Samples : cap.ch2Samples;
  if (!samples || samples.length === 0) return null;
  return computeFftSpectrum(samples, Number(cap.sampleRateHz), 1024);
});

const extendedMetrics = computed(() => {
  if (!lastCapture.value || !lastCapture.value.ch1Samples || lastCapture.value.ch1Samples.length === 0) return null;
  const cap = lastCapture.value;
  const timePerSampleUs = (parseFloat(timebaseDivUs.value) * 10.0) / (cap.samplesCount || 5000);
  return computeExtendedMetrics(
    cap.ch1Samples,
    timePerSampleUs,
    cap.ch2Samples && cap.ch2Samples.length > 0 ? cap.ch2Samples : undefined
  );
});

function toggleMath() {
  if (!mathEnabled.value) {
    mathEnabled.value = true;
    mathOp.value = 'CH1-CH2';
  } else if (mathOp.value === 'CH1-CH2') {
    mathOp.value = 'CH1+CH2';
  } else if (mathOp.value === 'CH1+CH2') {
    mathOp.value = 'INV_CH1';
  } else {
    mathEnabled.value = false;
    mathOp.value = 'CH1-CH2';
  }
}

function handleExportCsv() {
  if (!lastCapture.value) return;
  const cap = lastCapture.value;
  const timePerSampleUs = (parseFloat(timebaseDivUs.value) * 10.0) / (cap.samplesCount || 5000);
  exportCaptureCsv(cap.ch1Samples, cap.ch2Samples, timePerSampleUs);
}

function handleExportJson() {
  if (!lastCapture.value) return;
  exportCaptureJson(lastCapture.value);
}

const calChannelOptions = [
  { label: 'CH1', value: 'CH1' },
  { label: 'CH2', value: 'CH2' },
];

const calProbeOptions = [
  { label: '1X', value: '1' },
  { label: '10X', value: '10' },
];

// Computed Cursor measurements
const deltaTUs = computed(() => Math.abs(cursorTime2Us.value - cursorTime1Us.value));
const cursorFreqHz = computed(() => {
  if (deltaTUs.value <= 0) return null;
  return 1000000 / deltaTUs.value;
});
const deltaVMv = computed(() => Math.abs(cursorVolt2Mv.value - cursorVolt1Mv.value));

function formatVoltsPerDiv(mvStr: string): string {
  const mv = parseInt(mvStr, 10);
  if (mv >= 1000) return `${(mv / 1000).toFixed(mv % 1000 === 0 ? 0 : 1)} V/div`;
  return `${mv} mV/div`;
}

function formatTimebase(usStr: string): string {
  const us = parseFloat(usStr);
  if (us < 1) return `${Math.round(us * 1000)} ns/div`;
  if (us >= 1000) return `${(us / 1000).toFixed(us % 1000 === 0 ? 0 : 1)} ms/div`;
  return `${Math.round(us)} µs/div`;
}

async function handleProbeCheck() {
  if (isCalibrating.value) return;
  isCalibrating.value = true;
  calError.value = null;
  calResult.value = null;

  try {
    const res = await daemonCall('calibrate_oscilloscope', {
      mode: 'probe_check',
      channel: calChannel.value,
      probeAttenuation: parseInt(calProbeAtten.value, 10),
    });
    calResult.value = res;
    if (res.status === 'passed' && res.measuredFreqHz != null) {
      lastCalSummary.value = `${(res.measuredFreqHz / 1000).toFixed(2)} kHz OK`;
    }
    // Automatically trigger a capture so the waveform is rendered on screen
    await store.performCapture();
  } catch (err: unknown) {
    calError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isCalibrating.value = false;
  }
}

async function handleZeroOffsetCalibration() {
  if (isCalibrating.value) return;
  isCalibrating.value = true;
  calError.value = null;
  calResult.value = null;

  try {
    const res = await daemonCall('calibrate_oscilloscope', {
      mode: 'zero_offset',
      channel: null,
      probeAttenuation: null,
    });
    calResult.value = res;
    if (res.status === 'passed') {
      lastCalSummary.value = 'Zero Calibrated';
    }
  } catch (err: unknown) {
    calError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isCalibrating.value = false;
  }
}

function copyAgentDigest() {
  if (!lastCapture.value) return;
  const digest = {
    ch1Metrics: lastCapture.value.ch1Metrics,
    ch2Metrics: lastCapture.value.ch2Metrics,
    timePerDivUs: lastCapture.value.timePerDivUs,
    sampleRateHz: Number(lastCapture.value.sampleRateHz),
    samplesCount: lastCapture.value.samplesCount,
    durationMs: Number(lastCapture.value.durationMs),
  };
  navigator.clipboard.writeText(JSON.stringify(digest, null, 2));
  copiedDigest.value = true;
  setTimeout(() => {
    copiedDigest.value = false;
  }, 2000);
}

// Oscilloscope Canvas Rendering Engine
function drawOscilloscope() {
  const canvas = canvasRef.value;
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const dpr = window.devicePixelRatio || 1;
  const width = canvas.width / dpr;
  const height = canvas.height / dpr;
  if (width === 0 || height === 0) return;

  ctx.save();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  // Clear background
  ctx.fillStyle = '#090a0f';
  ctx.fillRect(0, 0, width, height);

  const xDivs = 10;
  const dx = width / xDivs;
  const centerX = width / 2;

  if (displayMode.value === 'split') {
    // --- SPLIT MODE (Dual Stacked Grids) ---
    const halfHeight = height / 2;
    const yDivs = 8;
    const dySplit = halfHeight / yDivs;
    const centerY1 = halfHeight / 2;
    const centerY2 = halfHeight + halfHeight / 2;

    // Grid background lines
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.07)';
    ctx.lineWidth = 1;

    // Vertical lines across both viewports
    for (let i = 0; i <= xDivs; i++) {
      const x = i * dx;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }

    // Horizontal lines in top half (CH1)
    for (let j = 0; j <= yDivs; j++) {
      const y = j * dySplit;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    // Horizontal lines in bottom half (CH2)
    for (let j = 0; j <= yDivs; j++) {
      const y = halfHeight + j * dySplit;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    // Center crosshairs for top half (CH1)
    ctx.strokeStyle = 'rgba(234, 179, 8, 0.2)';
    ctx.beginPath();
    ctx.moveTo(0, centerY1);
    ctx.lineTo(width, centerY1);
    ctx.stroke();

    // Center crosshairs for bottom half (CH2)
    ctx.strokeStyle = 'rgba(6, 182, 212, 0.2)';
    ctx.beginPath();
    ctx.moveTo(0, centerY2);
    ctx.lineTo(width, centerY2);
    ctx.stroke();

    // Horizontal Split Divider Line
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.35)';
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.beginPath();
    ctx.moveTo(0, halfHeight);
    ctx.lineTo(width, halfHeight);
    ctx.stroke();
    ctx.setLineDash([]);

    // Grid Viewport Labels
    ctx.font = 'bold 9px monospace';
    ctx.fillStyle = '#eab308';
    ctx.fillText('CH1 (TOP)', 8, 14);
    ctx.fillStyle = '#06b6d4';
    ctx.fillText('CH2 (BOTTOM)', 8, halfHeight + 14);

    // Ground References
    if (ch1Enabled.value) {
      const ch1VDiv = parseFloat(ch1VoltsPerDiv.value);
      const ch1ZeroY = centerY1 - (ch1OffsetMv.value / ch1VDiv) * dySplit;
      drawGndMarker(ctx, 4, Math.max(6, Math.min(halfHeight - 6, ch1ZeroY)), '#eab308', '1');
    }
    if (ch2Enabled.value) {
      const ch2VDiv = parseFloat(ch2VoltsPerDiv.value);
      const ch2ZeroY = centerY2 - (ch2OffsetMv.value / ch2VDiv) * dySplit;
      drawGndMarker(ctx, 4, Math.max(halfHeight + 6, Math.min(height - 6, ch2ZeroY)), '#06b6d4', '2');
    }

    // Trigger Level Marker
    if (triggerSource.value === 'CH2') {
      const trgVDiv = parseFloat(ch2VoltsPerDiv.value);
      const triggerY = centerY2 - ((triggerLevelMv.value + ch2OffsetMv.value) / trgVDiv) * dySplit;
      drawTriggerMarker(ctx, width - 4, Math.max(halfHeight + 6, Math.min(height - 6, triggerY)), '#f97316');
    } else {
      const trgVDiv = parseFloat(ch1VoltsPerDiv.value);
      const triggerY = centerY1 - ((triggerLevelMv.value + ch1OffsetMv.value) / trgVDiv) * dySplit;
      drawTriggerMarker(ctx, width - 4, Math.max(6, Math.min(halfHeight - 6, triggerY)), '#f97316');
    }

    // Render Waveforms
    if (lastCapture.value) {
      const capture = lastCapture.value;
      if (ch1Enabled.value && capture.ch1Samples && capture.ch1Samples.length > 0) {
        renderWaveform(
          ctx,
          capture.ch1Samples,
          width,
          halfHeight,
          0,
          centerY1,
          dySplit,
          parseFloat(ch1VoltsPerDiv.value),
          ch1OffsetMv.value,
          '#eab308'
        );
      }
      if (ch2Enabled.value && capture.ch2Samples && capture.ch2Samples.length > 0) {
        renderWaveform(
          ctx,
          capture.ch2Samples,
          width,
          halfHeight,
          halfHeight,
          centerY2,
          dySplit,
          parseFloat(ch2VoltsPerDiv.value),
          ch2OffsetMv.value,
          '#06b6d4'
        );
      }
    }

    // Draw Cursors
    if (showTimeCursors.value) {
      drawTimeCursor(ctx, width, height, centerX, dx, cursorTime1Us.value, '#a855f7', 'T1', hoveredCursor.value === 'T1' || activeDragCursor.value === 'T1');
      drawTimeCursor(ctx, width, height, centerX, dx, cursorTime2Us.value, '#ec4899', 'T2', hoveredCursor.value === 'T2' || activeDragCursor.value === 'T2');
    }
    if (showVoltCursors.value) {
      drawVoltageCursor(ctx, width, halfHeight, centerY1, dySplit, cursorVolt1Mv.value, '#84cc16', 'V1', hoveredCursor.value === 'V1' || activeDragCursor.value === 'V1');
      drawVoltageCursor(ctx, width, halfHeight, centerY1, dySplit, cursorVolt2Mv.value, '#10b981', 'V2', hoveredCursor.value === 'V2' || activeDragCursor.value === 'V2');
    }
  } else {
    // --- OVERLAY MODE (Single Shared Grid) ---
    const yDivs = 8;
    const dy = height / yDivs;
    const centerY = height / 2;

    // Grid background lines
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
    ctx.lineWidth = 1;

    for (let i = 0; i <= xDivs; i++) {
      const x = i * dx;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }

    for (let j = 0; j <= yDivs; j++) {
      const y = j * dy;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    // Center crosshairs with sub-division ticks (5 sub-ticks per div)
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.22)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(centerX, 0);
    ctx.lineTo(centerX, height);
    ctx.moveTo(0, centerY);
    ctx.lineTo(width, centerY);
    ctx.stroke();

    const tickSize = 3;
    for (let i = 0; i <= xDivs * 5; i++) {
      const x = (i * dx) / 5;
      ctx.beginPath();
      ctx.moveTo(x, centerY - tickSize);
      ctx.lineTo(x, centerY + tickSize);
      ctx.stroke();
    }
    for (let j = 0; j <= yDivs * 5; j++) {
      const y = (j * dy) / 5;
      ctx.beginPath();
      ctx.moveTo(centerX - tickSize, y);
      ctx.lineTo(centerX + tickSize, y);
      ctx.stroke();
    }

    // Ground References
    if (ch1Enabled.value) {
      const ch1VDiv = parseFloat(ch1VoltsPerDiv.value);
      const ch1ZeroY = centerY - (ch1OffsetMv.value / ch1VDiv) * dy;
      drawGndMarker(ctx, 4, ch1ZeroY, '#eab308', '1');
    }
    if (ch2Enabled.value) {
      const ch2VDiv = parseFloat(ch2VoltsPerDiv.value);
      const ch2ZeroY = centerY - (ch2OffsetMv.value / ch2VDiv) * dy;
      drawGndMarker(ctx, 4, ch2ZeroY, '#06b6d4', '2');
    }

    // Trigger Marker
    const trgVDiv = triggerSource.value === 'CH2' ? parseFloat(ch2VoltsPerDiv.value) : parseFloat(ch1VoltsPerDiv.value);
    const trgOffsetY = triggerSource.value === 'CH2' ? ch2OffsetMv.value : ch1OffsetMv.value;
    const triggerY = centerY - ((triggerLevelMv.value + trgOffsetY) / trgVDiv) * dy;
    drawTriggerMarker(ctx, width - 4, triggerY, '#f97316');

    // Render Waveforms
    if (lastCapture.value) {
      const capture = lastCapture.value;
      if (ch2Enabled.value && capture.ch2Samples && capture.ch2Samples.length > 0) {
        renderWaveform(
          ctx,
          capture.ch2Samples,
          width,
          height,
          0,
          centerY,
          dy,
          parseFloat(ch2VoltsPerDiv.value),
          ch2OffsetMv.value,
          '#06b6d4'
        );
      }
      if (ch1Enabled.value && capture.ch1Samples && capture.ch1Samples.length > 0) {
        renderWaveform(
          ctx,
          capture.ch1Samples,
          width,
          height,
          0,
          centerY,
          dy,
          parseFloat(ch1VoltsPerDiv.value),
          ch1OffsetMv.value,
          '#eab308'
        );
      }
      if (mathEnabled.value && capture.ch1Samples && capture.ch1Samples.length > 0) {
        const mathSamples = computeMathTrace(capture.ch1Samples, capture.ch2Samples || [], mathOp.value);
        renderWaveform(
          ctx,
          mathSamples,
          width,
          height,
          0,
          centerY,
          dy,
          parseFloat(ch1VoltsPerDiv.value),
          ch1OffsetMv.value,
          '#d946ef'
        );
        const mathZeroY = centerY - (ch1OffsetMv.value / parseFloat(ch1VoltsPerDiv.value)) * dy;
        drawGndMarker(ctx, 4, mathZeroY, '#d946ef', 'M');
      }
    }

    // Draw Cursors
    if (showTimeCursors.value) {
      drawTimeCursor(ctx, width, height, centerX, dx, cursorTime1Us.value, '#a855f7', 'T1', hoveredCursor.value === 'T1' || activeDragCursor.value === 'T1');
      drawTimeCursor(ctx, width, height, centerX, dx, cursorTime2Us.value, '#ec4899', 'T2', hoveredCursor.value === 'T2' || activeDragCursor.value === 'T2');
    }
    if (showVoltCursors.value) {
      drawVoltageCursor(ctx, width, height, centerY, dy, cursorVolt1Mv.value, '#84cc16', 'V1', hoveredCursor.value === 'V1' || activeDragCursor.value === 'V1');
      drawVoltageCursor(ctx, width, height, centerY, dy, cursorVolt2Mv.value, '#10b981', 'V2', hoveredCursor.value === 'V2' || activeDragCursor.value === 'V2');
    }
  }

  ctx.restore();
  drawFft();
}

function drawFft() {
  if (!showFft.value || !fftCanvasRef.value || !lastCapture.value) return;
  const canvas = fftCanvasRef.value;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const width = rect.width;
  const height = rect.height;
  if (width <= 0 || height <= 0) return;

  canvas.width = Math.floor(width * dpr);
  canvas.height = Math.floor(height * dpr);
  ctx.scale(dpr, dpr);

  ctx.fillStyle = '#06070c';
  ctx.fillRect(0, 0, width, height);

  const cap = lastCapture.value;
  const samples = cap.ch1Samples && cap.ch1Samples.length > 0 ? cap.ch1Samples : cap.ch2Samples;
  if (!samples || samples.length === 0) return;

  const fft = computeFftSpectrum(samples, Number(cap.sampleRateHz), 1024);
  if (!fft || fft.magnitudesDb.length === 0) return;

  const dbMin = -90;
  const dbMax = 10;
  const dbRange = dbMax - dbMin;

  // Horizontal Grid Lines
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
  ctx.lineWidth = 1;
  ctx.font = '8px monospace';
  ctx.fillStyle = '#71717a';

  const dbTicks = [-80, -60, -40, -20, 0];
  for (const db of dbTicks) {
    const y = height - ((db - dbMin) / dbRange) * height;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
    ctx.fillText(`${db}dB`, 4, y - 2);
  }

  // Vertical Frequency Grid Lines
  for (let f = 1; f <= 5; f++) {
    const x = (f / 6) * width;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
    const freqAtX = (f / 6) * fft.nyquistHz;
    const freqLabel = freqAtX >= 1e6 ? `${(freqAtX / 1e6).toFixed(1)}M` : `${Math.round(freqAtX / 1e3)}k`;
    ctx.fillText(freqLabel, x + 2, height - 4);
  }

  // Draw spectrum trace
  ctx.strokeStyle = '#a855f7';
  ctx.lineWidth = 1.5;
  ctx.beginPath();

  const count = fft.magnitudesDb.length;
  const stepX = width / (count - 1);

  for (let i = 0; i < count; i++) {
    const x = i * stepX;
    const db = Math.max(dbMin, Math.min(dbMax, fft.magnitudesDb[i]));
    const y = height - ((db - dbMin) / dbRange) * height;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();

  // Peak annotation
  if (fft.peakFreqHz > 0) {
    const peakX = (fft.peakFreqHz / fft.nyquistHz) * width;
    const peakY = height - ((fft.peakMagnitudeDb - dbMin) / dbRange) * height;
    ctx.fillStyle = '#f59e0b';
    ctx.beginPath();
    ctx.arc(peakX, peakY, 3, 0, Math.PI * 2);
    ctx.fill();
    const label = `${(fft.peakFreqHz / 1000).toFixed(1)}kHz (${fft.peakMagnitudeDb.toFixed(1)}dB)`;
    ctx.fillText(label, Math.min(peakX + 5, width - 75), Math.max(peakY - 5, 12));
  }
}

function drawGndMarker(ctx: CanvasRenderingContext2D, x: number, y: number, color: string, label: string) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.moveTo(x, y - 5);
  ctx.lineTo(x + 10, y);
  ctx.lineTo(x, y + 5);
  ctx.closePath();
  ctx.fill();

  ctx.fillStyle = '#000000';
  ctx.font = 'bold 8px monospace';
  ctx.fillText(label, x + 2, y + 2.5);
}

function drawTriggerMarker(ctx: CanvasRenderingContext2D, x: number, y: number, color: string) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.moveTo(x, y - 5);
  ctx.lineTo(x - 10, y);
  ctx.lineTo(x, y + 5);
  ctx.closePath();
  ctx.fill();

  ctx.fillStyle = '#000000';
  ctx.font = 'bold 8px monospace';
  ctx.fillText('T', x - 7, y + 2.5);
}

function renderWaveform(
  ctx: CanvasRenderingContext2D,
  samples: number[],
  width: number,
  height: number,
  topY: number,
  centerY: number,
  dy: number,
  voltsPerDivMv: number,
  offsetMv: number,
  color: string
) {
  if (samples.length < 2) return;

  ctx.save();
  ctx.beginPath();
  ctx.rect(0, topY, width, height);
  ctx.clip();

  ctx.strokeStyle = color;
  ctx.lineWidth = 1.75;
  ctx.lineJoin = 'round';
  ctx.beginPath();

  const stepX = width / (samples.length - 1);

  for (let i = 0; i < samples.length; i++) {
    const x = i * stepX;
    // Samples are in mV. Screen Y = centerY - ((voltageMv + offsetMv) / voltsPerDivMv) * dy
    const y = centerY - ((samples[i] + offsetMv) / voltsPerDivMv) * dy;

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }

  ctx.stroke();
  ctx.restore();
}

function drawTimeCursor(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  centerX: number,
  dx: number,
  timeUs: number,
  color: string,
  label: string,
  isHighlighted: boolean = false
) {
  const tDiv = parseFloat(timebaseDivUs.value);
  const x = centerX + (timeUs / tDiv) * dx;
  if (x < 0 || x > width) return;

  ctx.save();
  ctx.strokeStyle = color;
  ctx.setLineDash(isHighlighted ? [] : [4, 4]);
  ctx.lineWidth = isHighlighted ? 2.0 : 1.2;
  ctx.beginPath();
  ctx.moveTo(x, 0);
  ctx.lineTo(x, height);
  ctx.stroke();

  // Top Handle Badge
  ctx.fillStyle = color;
  ctx.fillRect(x - 14, 2, 28, 14);
  ctx.fillStyle = '#000000';
  ctx.font = 'bold 9px monospace';
  ctx.textAlign = 'center';
  ctx.fillText(label, x, 12);

  // Value text
  ctx.fillStyle = color;
  ctx.textAlign = 'left';
  ctx.font = isHighlighted ? 'bold 10px monospace' : '10px monospace';
  ctx.fillText(`${timeUs}µs`, x + 18, 13);
  ctx.restore();
}

function drawVoltageCursor(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  centerY: number,
  dy: number,
  voltageMv: number,
  color: string,
  label: string,
  isHighlighted: boolean = false
) {
  const vDiv = parseFloat(ch1VoltsPerDiv.value);
  const y = centerY - (voltageMv / vDiv) * dy;
  if (y < 0 || y > height) return;

  ctx.save();
  ctx.strokeStyle = color;
  ctx.setLineDash(isHighlighted ? [] : [4, 4]);
  ctx.lineWidth = isHighlighted ? 2.0 : 1.2;
  ctx.beginPath();
  ctx.moveTo(0, y);
  ctx.lineTo(width, y);
  ctx.stroke();

  // Left Handle Badge
  ctx.fillStyle = color;
  ctx.fillRect(2, y - 7, 24, 14);
  ctx.fillStyle = '#000000';
  ctx.font = 'bold 9px monospace';
  ctx.textAlign = 'center';
  ctx.fillText(label, 14, y + 3);

  // Value text
  ctx.fillStyle = color;
  ctx.textAlign = 'left';
  ctx.font = isHighlighted ? 'bold 10px monospace' : '10px monospace';
  const voltStr = Math.abs(voltageMv) >= 1000 ? `${(voltageMv / 1000).toFixed(2)}V` : `${voltageMv}mV`;
  ctx.fillText(voltStr, 30, y + 3.5);
  ctx.restore();
}

function findClosestCursor(
  mouseX: number,
  mouseY: number,
  width: number,
  height: number
): DragCursorType {
  const isSplit = displayMode.value === 'split';
  const centerX = width / 2;
  const dx = width / 10;
  const dy = isSplit ? height / 16 : height / 8;
  const centerY = isSplit ? height / 4 : height / 2;
  const tDiv = parseFloat(timebaseDivUs.value);
  const vDiv = parseFloat(ch1VoltsPerDiv.value);

  const threshold = 10;
  let closest: DragCursorType = null;
  let minDistance = threshold;

  if (showTimeCursors.value) {
    const xT1 = centerX + (cursorTime1Us.value / tDiv) * dx;
    const distT1 = Math.abs(mouseX - xT1);
    if (distT1 < minDistance) {
      minDistance = distT1;
      closest = 'T1';
    }

    const xT2 = centerX + (cursorTime2Us.value / tDiv) * dx;
    const distT2 = Math.abs(mouseX - xT2);
    if (distT2 < minDistance) {
      minDistance = distT2;
      closest = 'T2';
    }
  }

  if (showVoltCursors.value) {
    const yV1 = centerY - (cursorVolt1Mv.value / vDiv) * dy;
    const distV1 = Math.abs(mouseY - yV1);
    if (distV1 < minDistance) {
      minDistance = distV1;
      closest = 'V1';
    }

    const yV2 = centerY - (cursorVolt2Mv.value / vDiv) * dy;
    const distV2 = Math.abs(mouseY - yV2);
    if (distV2 < minDistance) {
      minDistance = distV2;
      closest = 'V2';
    }
  }

  return closest;
}

function handleCanvasPointerDown(e: PointerEvent) {
  if (e.button !== 0) return;
  const canvas = canvasRef.value;
  if (!canvas) return;

  const rect = canvas.getBoundingClientRect();
  const mouseX = e.clientX - rect.left;
  const mouseY = e.clientY - rect.top;

  const hit = findClosestCursor(mouseX, mouseY, rect.width, rect.height);
  if (hit) {
    activeDragCursor.value = hit;
    canvas.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
}

function handleCanvasPointerMove(e: PointerEvent) {
  const canvas = canvasRef.value;
  if (!canvas) return;

  const rect = canvas.getBoundingClientRect();
  const mouseX = e.clientX - rect.left;
  const mouseY = e.clientY - rect.top;
  const width = rect.width;
  const height = rect.height;

  if (activeDragCursor.value) {
    const isSplit = displayMode.value === 'split';
    const centerX = width / 2;
    const dx = width / 10;
    const dy = isSplit ? height / 16 : height / 8;
    const centerY = isSplit ? height / 4 : height / 2;
    const tDiv = parseFloat(timebaseDivUs.value);
    const vDiv = parseFloat(ch1VoltsPerDiv.value);
    const maxT = Math.round(5 * tDiv);
    const maxV = Math.round(5 * vDiv);

    if (activeDragCursor.value === 'T1') {
      const newT = Math.round(((mouseX - centerX) / dx) * tDiv);
      store.cursorTime1Us = Math.max(-maxT, Math.min(maxT, newT));
    } else if (activeDragCursor.value === 'T2') {
      const newT = Math.round(((mouseX - centerX) / dx) * tDiv);
      store.cursorTime2Us = Math.max(-maxT, Math.min(maxT, newT));
    } else if (activeDragCursor.value === 'V1') {
      const newV = Math.round(((centerY - mouseY) / dy) * vDiv);
      store.cursorVolt1Mv = Math.max(-maxV, Math.min(maxV, newV));
    } else if (activeDragCursor.value === 'V2') {
      const newV = Math.round(((centerY - mouseY) / dy) * vDiv);
      store.cursorVolt2Mv = Math.max(-maxV, Math.min(maxV, newV));
    }
  } else {
    hoveredCursor.value = findClosestCursor(mouseX, mouseY, width, height);
  }
}

function handleCanvasPointerUp(e: PointerEvent) {
  if (activeDragCursor.value) {
    const canvas = canvasRef.value;
    if (canvas && canvas.hasPointerCapture(e.pointerId)) {
      canvas.releasePointerCapture(e.pointerId);
    }
    activeDragCursor.value = null;
  }
}

function handleCanvasPointerLeave() {
  if (!activeDragCursor.value) {
    hoveredCursor.value = null;
  }
}

function handleCanvasDoubleClick(e: MouseEvent) {
  const canvas = canvasRef.value;
  if (!canvas) return;

  const rect = canvas.getBoundingClientRect();
  const mouseX = e.clientX - rect.left;
  const mouseY = e.clientY - rect.top;

  const hit = findClosestCursor(mouseX, mouseY, rect.width, rect.height);
  if (hit === 'T1') {
    store.cursorTime1Us = 0;
  } else if (hit === 'T2') {
    store.cursorTime2Us = 0;
  } else if (hit === 'V1') {
    store.cursorVolt1Mv = 0;
  } else if (hit === 'V2') {
    store.cursorVolt2Mv = 0;
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
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    drawOscilloscope();
  }
}

function formatFrequency(freqHz: number | null): string {
  if (freqHz === null || isNaN(freqHz)) return '---';
  if (freqHz >= 1000000) return `${(freqHz / 1000000).toFixed(3)} MHz`;
  if (freqHz >= 1000) return `${(freqHz / 1000).toFixed(2)} kHz`;
  return `${freqHz.toFixed(1)} Hz`;
}

function formatVoltage(mV: number): string {
  if (isNaN(mV)) return '---';
  if (Math.abs(mV) >= 1000) return `${(mV / 1000).toFixed(2)} V`;
  return `${mV.toFixed(0)} mV`;
}

onMounted(() => {
  store.scanHardware();
  if (containerRef.value) {
    resizeObserver = new ResizeObserver(() => {
      resizeCanvas();
    });
    resizeObserver.observe(containerRef.value);
  }
  // Draw existing capture from store immediately, or capture if idle and empty
  if (lastCapture.value) {
    resizeCanvas();
  } else if (!isContinuousRunning.value) {
    store.performCapture();
  }
  unlistenSurfaceCmd = onDaemonEvent('ui-surface-command', (payload) => {
    if (payload.projectId !== props.projectId || payload.nodePath !== 'electronics/oscilloscope') {
      return;
    }
    if (payload.actionId === 'set_channel_enabled') {
      const params = payload.params as { channel?: number; enabled?: boolean } | null;
      if (params?.channel === 1 && typeof params.enabled === 'boolean') {
        ch1Enabled.value = params.enabled;
      } else if (params?.channel === 2 && typeof params.enabled === 'boolean') {
        ch2Enabled.value = params.enabled;
      }
    } else if (payload.actionId === 'set_timebase') {
      const params = payload.params as { timebase_us?: string | number } | null;
      if (params?.timebase_us !== undefined && params.timebase_us !== null) {
        timebaseDivUs.value = String(params.timebase_us);
      }
    } else if (payload.actionId === 'set_trigger') {
      const params = payload.params as { source?: string; slope?: string; sweep?: string } | null;
      if (params?.source) triggerSource.value = params.source;
      if (params?.slope) triggerSlope.value = params.slope;
      if (params?.sweep) triggerSweep.value = params.sweep;
    } else if (payload.actionId === 'capture_single') {
      store.triggerSingleCapture();
    }
  });
});

let unlistenSurfaceCmd: (() => void) | null = null;

onUnmounted(() => {
  // Retain continuous acquisition running in store across tab navigation
  if (unlistenSurfaceCmd) {
    unlistenSurfaceCmd();
    unlistenSurfaceCmd = null;
  }
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});

// Redraw whenever visual configurations or new capture frames arrive
watch(
  [
    ch1Enabled,
    ch1VoltsPerDiv,
    ch1OffsetMv,
    ch2Enabled,
    ch2VoltsPerDiv,
    ch2OffsetMv,
    timebaseDivUs,
    triggerLevelMv,
    showTimeCursors,
    showVoltCursors,
    cursorTime1Us,
    cursorTime2Us,
    cursorVolt1Mv,
    cursorVolt2Mv,
    displayMode,
    mathEnabled,
    mathOp,
    showFft,
    lastCapture,
    hoveredCursor,
    activeDragCursor,
  ],
  () => {
    drawOscilloscope();
  }
);

// Immediately perform hardware acquisition when channel enabled states toggle
watch([ch1Enabled, ch2Enabled], () => {
  store.performCapture();
});

watch(showFft, async (val) => {
  if (val) {
    await nextTick();
    drawFft();
  }
});
</script>

<template>
  <div
    class="h-full flex flex-col min-w-0 bg-[var(--bg-color)] text-[var(--fg-color)] overflow-hidden"
    style="--osc-ch1: #eab308; --osc-ch2: #06b6d4; --osc-trig: #f97316; --osc-math: #e879f9; --osc-t1: #c084fc; --osc-t2: #f472b6; --osc-v1: #a3e635; --osc-v2: #34d399;"
  >
    <!-- Oscilloscope Top Toolbar -->
    <div class="h-9 ui-surface-title border-b border-[var(--border-color)] px-3 flex items-center justify-between gap-2 shrink-0 overflow-x-auto">
      <div class="flex items-center gap-2 shrink-0">
        <Activity class="w-3.5 h-3.5 text-[var(--status-warn-color)] shrink-0" />
        <div class="w-52">
          <AppSelect
            v-model="selectedScopeId"
            :options="scopeModels"
          />
        </div>

        <!-- Hardware Status Badge -->
        <span
          class="text-[9px] font-mono px-1.5 py-0.5 border"
          :class="connectedOwon?.isConnected
            ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold'
            : 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)]'"
        >
          {{ connectedOwon?.isConnected ? 'Connected' : 'Offline' }}
        </span>

        <span
          v-if="lastCalSummary"
          class="text-[9px] font-mono px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold"
        >
          CAL: {{ lastCalSummary }}
        </span>

        <AppTooltip text="Scan for USB oscilloscope hardware">
          <button
            @click="store.scanHardware"
            :disabled="isScanning"
            class="ui-icon-btn !h-6 !w-6 disabled:opacity-50"
          >
            <RotateCw class="w-3 h-3" :class="{ 'animate-spin': isScanning }" />
          </button>
        </AppTooltip>
      </div>

      <!-- Capture Controls -->
      <div class="flex items-center gap-1.5 shrink-0">
        <!-- Master Auto Set Button (Icon Only) -->
        <AppTooltip text="Auto Set: Scale voltage, timebase, and center waveform">
          <button
            @click="store.autoSetOscilloscope"
            :disabled="!connectedOwon?.isConnected || isAutoSetting"
            class="ui-icon-btn !h-6 !w-6 !border-[var(--status-warn-border)] !bg-[var(--status-warn-bg)] !text-[var(--status-warn-color)] hover:opacity-90 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Loader2 v-if="isAutoSetting" class="w-3.5 h-3.5 animate-spin" />
            <Sparkles v-else class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <!-- Display Mode Toggle (Icon Only) -->
        <AppTooltip :text="displayMode === 'split' ? 'Dual Grid (Split CH1/CH2)' : 'Single Grid (Overlaid)'">
          <button
            @click="store.toggleDisplayMode"
            :disabled="!connectedOwon?.isConnected"
            class="ui-icon-btn !h-6 !w-6 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Rows2 v-if="displayMode === 'split'" class="w-3.5 h-3.5 text-[var(--accent-color)]" />
            <LayoutGrid v-else class="w-3.5 h-3.5 text-[var(--status-warn-color)]" />
          </button>
        </AppTooltip>

        <div class="h-4 w-px bg-[var(--border-color)] mx-0.5" />

        <!-- Run / Stop (Icon Only) -->
        <AppTooltip :text="isContinuousRunning ? 'Stop continuous acquisition' : 'Run continuous acquisition'">
          <button
            v-if="!isContinuousRunning"
            @click="store.startContinuous"
            :disabled="!connectedOwon?.isConnected || isCapturing"
            class="ui-icon-btn !h-6 !w-6 !border-[var(--status-ok-border)] !bg-[var(--status-ok-bg)] !text-[var(--status-ok-color)] hover:opacity-90 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Play class="w-3.5 h-3.5" />
          </button>
          <button
            v-else
            @click="store.stopContinuous"
            :disabled="!connectedOwon?.isConnected"
            class="ui-icon-btn !h-6 !w-6 !border-[var(--status-warn-border)] !bg-[var(--status-warn-bg)] !text-[var(--status-warn-color)] hover:opacity-90 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Square class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <!-- Single Sweep (Icon Only) -->
        <AppTooltip text="Capture single frame (Single sweep)">
          <button
            @click="store.triggerSingleCapture"
            :disabled="!connectedOwon?.isConnected || isCapturing || isContinuousRunning"
            class="ui-icon-btn !h-6 !w-6 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Zap class="w-3.5 h-3.5" />
          </button>
        </AppTooltip>

        <!-- Calibrate (Icon Only, Conditional by Hardware Capability) -->
        <AppTooltip
          v-if="activeScopeDescriptor?.hasHardwareCalibration"
          text="Open probe compensation and calibration wizard"
        >
          <button
            @click="showCalibrationModal = true"
            :disabled="!connectedOwon?.isConnected"
            class="ui-icon-btn !h-6 !w-6 disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Wrench class="w-3.5 h-3.5 text-[var(--status-warn-color)]" />
          </button>
        </AppTooltip>

        <div class="h-4 w-px bg-[var(--border-color)] mx-0.5" />

        <!-- Timebase Stepper (Integrated from bottom bar) -->
        <div
          class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)] font-mono text-[11px]"
          :class="{ 'opacity-30 pointer-events-none': !connectedOwon?.isConnected }"
        >
          <AppTooltip text="Faster timebase">
            <button
              type="button"
              v-on="holdTimebaseDec"
              :disabled="!connectedOwon?.isConnected"
              class="px-1.5 py-0.5 border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:cursor-not-allowed"
            >
              -
            </button>
          </AppTooltip>
          <AppTooltip text="Horizontal timebase per division">
            <span class="w-16 text-center py-0.5 text-xs font-mono tabular-nums text-[var(--fg-color)] select-none">
              {{ formatTimebase(timebaseDivUs) }}
            </span>
          </AppTooltip>
          <AppTooltip text="Slower timebase">
            <button
              type="button"
              v-on="holdTimebaseInc"
              :disabled="!connectedOwon?.isConnected"
              class="px-1.5 py-0.5 border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:cursor-not-allowed"
            >
              +
            </button>
          </AppTooltip>
        </div>

        <!-- Cursor Toggles: Δt and ΔV (Symbols only) -->
        <div
          class="flex items-center border border-[var(--border-color)] font-mono text-xs"
          :class="{ 'opacity-30 pointer-events-none': !connectedOwon?.isConnected }"
        >
          <AppTooltip text="Toggle time cursors (T1/T2, Δt) - Drag lines on canvas, double click to zero">
            <button
              type="button"
              @click="showTimeCursors = !showTimeCursors"
              :disabled="!connectedOwon?.isConnected"
              class="px-2 py-0.5 font-bold transition-colors cursor-pointer disabled:cursor-not-allowed"
              :class="showTimeCursors
                ? 'bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold'
                : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Δt
            </button>
          </AppTooltip>
          <AppTooltip text="Toggle voltage cursors (V1/V2, ΔV) - Drag lines on canvas, double click to zero">
            <button
              type="button"
              @click="showVoltCursors = !showVoltCursors"
              :disabled="!connectedOwon?.isConnected"
              class="px-2 py-0.5 font-bold border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:cursor-not-allowed"
              :class="showVoltCursors
                ? 'bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold'
                : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              ΔV
            </button>
          </AppTooltip>
        </div>

        <div class="h-4 w-px bg-[var(--border-color)] mx-0.5" />

        <!-- Phase 2: Math Channel Button -->
        <AppTooltip text="Cycle math channel (CH1-CH2, CH1+CH2, INV_CH1, OFF)">
          <button
            @click="toggleMath"
            :disabled="!connectedOwon?.isConnected"
            class="h-6 px-2 text-[9px] font-mono uppercase border transition-all flex items-center gap-1 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
            :class="mathEnabled
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            <Sigma class="w-3 h-3" />
            <span>{{ mathEnabled ? mathOp : 'Math' }}</span>
          </button>
        </AppTooltip>

        <!-- Phase 2: FFT Spectrum Analyzer Toggle -->
        <AppTooltip text="Toggle real-time FFT spectrum analyzer">
          <button
            @click="showFft = !showFft"
            :disabled="!connectedOwon?.isConnected"
            class="h-6 px-2 text-[9px] font-mono uppercase border transition-all flex items-center gap-1 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
            :class="showFft
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            <BarChart2 class="w-3 h-3" />
            <span>FFT</span>
          </button>
        </AppTooltip>

        <!-- Phase 2: Extended Metrics Toggle -->
        <AppTooltip text="Toggle extended parametric measurements panel">
          <button
            @click="showExtendedMetrics = !showExtendedMetrics"
            :disabled="!connectedOwon?.isConnected"
            class="h-6 px-2 text-[9px] font-mono uppercase border transition-all flex items-center gap-1 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
            :class="showExtendedMetrics
              ? 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold'
              : 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            <span>Meas</span>
          </button>
        </AppTooltip>

        <!-- Phase 2: Waveform Export Buttons -->
        <div
          class="flex items-center border border-[var(--border-color)] font-mono text-[9px]"
          :class="{ 'opacity-30 pointer-events-none': !connectedOwon?.isConnected }"
        >
          <AppTooltip text="Export active waveform samples to CSV">
            <button
              @click="handleExportCsv"
              :disabled="!connectedOwon?.isConnected || !lastCapture"
              class="h-6 px-1.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1"
            >
              <Download class="w-2.5 h-2.5" />
              <span>CSV</span>
            </button>
          </AppTooltip>
          <AppTooltip text="Export full capture result and metadata to JSON">
            <button
              @click="handleExportJson"
              :disabled="!connectedOwon?.isConnected || !lastCapture"
              class="h-6 px-1.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)] border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              <span>JSON</span>
            </button>
          </AppTooltip>
        </div>

        <div class="h-4 w-px bg-[var(--border-color)] mx-0.5" />

        <!-- Agent Digest Button -->
        <AppTooltip text="Inspect structured metrics digest generated for AI agent debugging">
          <button
            @click="showAgentDigest = !showAgentDigest"
            :disabled="!connectedOwon?.isConnected"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] transition-colors flex items-center gap-1 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none"
          >
            <Code class="w-3 h-3" />
            <span>Digest</span>
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Error Alert Banner -->
    <div
      v-if="captureError && connectedOwon?.isConnected"
      class="px-4 py-2 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-xs font-mono flex items-center justify-between shrink-0"
    >
      <div class="flex items-center gap-2">
        <span class="font-bold uppercase tracking-wider">[Capture Error]</span>
        <span>{{ captureError }}</span>
      </div>
      <button
        @click="captureError = null"
        class="text-[var(--status-error-color)] text-xs uppercase font-bold px-2 py-0.5 border border-[var(--status-error-border)] transition-colors cursor-pointer"
      >
        Dismiss
      </button>
    </div>

    <!-- Main Oscilloscope Display & Knobs -->
    <div class="flex-1 flex flex-col xl:flex-row min-h-0 min-w-0 relative">
      <!-- Disconnected Global Overlay (Covers Entire Workstation: Canvas, FFT, Channels, Trigger) -->
      <div
        v-if="!connectedOwon?.isConnected"
        class="absolute inset-0 bg-[var(--bg-color)]/90 flex flex-col items-center justify-center text-center p-6 select-none z-30 pointer-events-auto"
      >
        <div class="p-3 border border-[var(--border-color)] bg-[var(--surface-bg)] mb-3">
          <Activity class="w-6 h-6 text-[var(--dim-color)]" />
        </div>
        <h3 class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)] mb-1">
          Oscilloscope Disconnected
        </h3>
        <p class="text-[11px] font-mono text-[var(--dim-color)] max-w-xs">
          Connect a supported device to start acquisition
        </p>
      </div>

      <!-- Center Work Area: Canvas + Bottom Tools -->
      <div class="flex-1 flex flex-col min-w-0 min-h-0 border-r border-[var(--border-color)] overflow-hidden">
        <!-- Canvas Display Area (100% full width of workstation center) -->
        <div ref="containerRef" class="flex-1 min-h-0 bg-[var(--ui-bg-base)] relative overflow-hidden select-none">
          <canvas
            ref="canvasRef"
            :style="{ cursor: connectedOwon?.isConnected ? canvasCursorStyle : 'default' }"
            @pointerdown="handleCanvasPointerDown"
            @pointermove="handleCanvasPointerMove"
            @pointerup="handleCanvasPointerUp"
            @pointercancel="handleCanvasPointerUp"
            @pointerleave="handleCanvasPointerLeave"
            @dblclick="handleCanvasDoubleClick"
            class="w-full h-full block"
          />

          <!-- Overlay HUD: Top info bar -->
          <div class="absolute top-2 left-3 right-3 flex items-center justify-between text-[11px] font-mono pointer-events-none">
            <div class="flex items-center gap-4 bg-[var(--ui-bg-title)]/90 px-2 py-1 border border-[var(--border-color)]">
              <span v-if="ch1Enabled" class="text-[var(--osc-ch1)] font-bold">
                CH1: {{ (parseFloat(ch1VoltsPerDiv) / 1000).toFixed(2) }}V/div ({{ ch1Coupling }})
              </span>
              <span v-if="ch2Enabled" class="text-[var(--osc-ch2)] font-bold">
                CH2: {{ (parseFloat(ch2VoltsPerDiv) / 1000).toFixed(2) }}V/div ({{ ch2Coupling }})
              </span>
              <span class="text-[var(--fg-color)]">
                TB: {{ timebaseDivUs }}µs/div
              </span>
            </div>

            <div class="flex items-center gap-3 bg-[var(--ui-bg-title)]/90 px-2 py-1 border border-[var(--border-color)]">
              <span class="text-[var(--status-warn-color)] font-bold">
                TRG: {{ triggerSource }} {{ triggerSlope === 'Rising' ? '↗' : '↘' }} ({{ triggerLevelMv }}mV)
              </span>
              <span v-if="lastCapture" class="text-[var(--dim-color)]">
                {{ lastCapture.samplesCount }} pts @ {{ (Number(lastCapture.sampleRateHz) / 1000000).toFixed(1) }}MS/s
              </span>
            </div>
          </div>

          <!-- Overlay Cursors HUD -->
          <div
            v-if="showTimeCursors || showVoltCursors"
            class="absolute bottom-2 left-3 bg-[var(--ui-bg-title)]/95 px-2.5 py-1.5 border border-[var(--border-color)] text-[11px] font-mono text-[var(--fg-color)] flex items-center gap-4 pointer-events-none"
          >
            <template v-if="showTimeCursors">
              <span class="text-[var(--osc-t1)]">T1: {{ cursorTime1Us }}µs</span>
              <span class="text-[var(--osc-t2)]">T2: {{ cursorTime2Us }}µs</span>
              <span class="font-bold text-[var(--fg-color)]">Δt: {{ deltaTUs }}µs</span>
              <span class="text-[var(--status-ok-color)] font-bold" v-if="cursorFreqHz">1/Δt: {{ formatFrequency(cursorFreqHz) }}</span>
            </template>
            <div v-if="showTimeCursors && showVoltCursors" class="w-px h-3 bg-[var(--border-color)]" />
            <template v-if="showVoltCursors">
              <span class="text-[var(--osc-v1)]">V1: {{ formatVoltage(cursorVolt1Mv) }}</span>
              <span class="text-[var(--osc-v2)]">V2: {{ formatVoltage(cursorVolt2Mv) }}</span>
              <span class="font-bold text-[var(--fg-color)]">ΔV: {{ formatVoltage(deltaVMv) }}</span>
            </template>
          </div>
        </div>



      <!-- FFT Spectrum Analyzer Canvas (Docked below bottom toolbar) -->
      <div v-if="showFft" class="h-32 border-t border-[var(--border-color)] bg-[var(--ui-bg-base)] relative flex flex-col shrink-0">
        <div class="px-3 py-1 bg-[var(--ui-bg-title)] border-b border-[var(--border-color)] flex items-center justify-between text-[10px] font-mono select-none">
          <div class="flex items-center gap-2">
            <span class="text-[var(--osc-t1)] font-bold uppercase tracking-wider">FFT Spectrum</span>
            <span v-if="fftResult && fftResult.peakFreqHz > 0" class="text-[var(--dim-color)]">
              Peak: <strong class="text-[var(--fg-color)]">{{ (fftResult.peakFreqHz / 1000).toFixed(2) }} kHz</strong>
              <span class="text-[var(--dim-color)] ml-1">({{ fftResult.peakMagnitudeDb.toFixed(1) }} dBV)</span>
            </span>
          </div>
          <span class="text-[var(--dim-color)]">1024-pt FFT • Hanning Window • Nyquist: {{ fftResult ? (fftResult.nyquistHz / 1000000).toFixed(1) + ' MHz' : '---' }}</span>
        </div>
        <canvas ref="fftCanvasRef" class="w-full flex-1 block" />
      </div>
    </div>

      <!-- Right: Front-Panel Knobs & Hardware Controls -->
      <div class="w-full xl:w-80 p-3 flex flex-col gap-3 bg-[var(--ui-bg-card)]/20 overflow-y-auto shrink-0 select-none">
        <!-- Channel 1 Controls Card -->
        <div class="border border-[var(--osc-ch1)]/40 bg-[var(--ui-bg-card)] p-2.5 flex flex-col gap-2">
          <!-- Section 1: State & Coupling Header -->
          <div class="flex items-center justify-between gap-1.5 border-b border-[var(--osc-ch1)]/30 pb-2">
            <div class="flex items-center gap-1.5 shrink-0">
              <AppToggle v-model="ch1Enabled" />
              <span class="text-xs font-bold uppercase text-[var(--osc-ch1)] tracking-wider">
                CH1
              </span>
            </div>

            <!-- Hardware Switches: 1X/10X + DC/AC/GND -->
            <div class="flex items-center gap-1 font-mono text-[10px]">
              <div class="flex items-center border border-[var(--border-color)]">
                <button
                  type="button"
                  @click="ch1ProbeAtten = '1'"
                  :disabled="!ch1Enabled"
                  class="px-1.5 py-0.5 text-center transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch1ProbeAtten === '1' ? 'bg-[var(--osc-ch1)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  1X
                </button>
                <button
                  type="button"
                  @click="ch1ProbeAtten = '10'"
                  :disabled="!ch1Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch1ProbeAtten === '10' ? 'bg-[var(--osc-ch1)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  10X
                </button>
              </div>

              <div class="flex items-center border border-[var(--border-color)]">
                <button
                  type="button"
                  @click="ch1Coupling = 'DC'"
                  :disabled="!ch1Enabled"
                  class="px-1.5 py-0.5 text-center transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch1Coupling === 'DC' ? 'bg-[var(--osc-ch1)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  DC
                </button>
                <button
                  type="button"
                  @click="ch1Coupling = 'AC'"
                  :disabled="!ch1Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch1Coupling === 'AC' ? 'bg-[var(--osc-ch1)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  AC
                </button>
                <button
                  type="button"
                  @click="ch1Coupling = 'GND'"
                  :disabled="!ch1Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch1Coupling === 'GND' ? 'bg-[var(--osc-ch1)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  GND
                </button>
              </div>
            </div>
          </div>

          <!-- Section 2: Steppers (Volts/Div & Offset) -->
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <label class="text-[9px] text-[var(--dim-color)] uppercase font-bold tracking-wider">Volts/Div</label>
                <AppTooltip text="Auto-scale V/div to fit signal">
                  <button
                    type="button"
                    @click="store.autoScaleChannel(1)"
                    :disabled="!ch1Enabled"
                    class="text-[9px] font-mono text-[var(--osc-ch1)]/80 hover:text-[var(--osc-ch1)] px-1 border border-transparent hover:border-[var(--osc-ch1)]/40 cursor-pointer disabled:opacity-40"
                  >
                    Fit
                  </button>
                </AppTooltip>
              </div>
              <div class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)]">
                <AppTooltip text="Decrease V/div">
                  <button
                    type="button"
                    v-on="holdCh1VDivDec"
                    :disabled="!ch1Enabled"
                    class="px-2 py-1 text-xs border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                  >
                    -
                  </button>
                </AppTooltip>
                <span class="w-full text-center py-1 text-xs font-mono tabular-nums text-[var(--fg-color)] select-none">
                  {{ formatVoltsPerDiv(ch1VoltsPerDiv) }}
                </span>
                <AppTooltip text="Increase V/div">
                  <button
                    type="button"
                    v-on="holdCh1VDivInc"
                    :disabled="!ch1Enabled"
                    class="px-2 py-1 text-xs border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                  >
                    +
                  </button>
                </AppTooltip>
              </div>
            </div>

            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <label class="text-[9px] text-[var(--dim-color)] uppercase font-bold tracking-wider">Offset</label>
                <div class="flex items-center gap-1">
                  <AppTooltip text="Auto-center curve on 0V reference line">
                    <button
                      type="button"
                      @click="store.autoCenterChannel(1)"
                      :disabled="!ch1Enabled"
                      class="text-[9px] font-mono text-[var(--osc-ch1)]/80 hover:text-[var(--osc-ch1)] px-1 border border-transparent hover:border-[var(--osc-ch1)]/40 cursor-pointer disabled:opacity-40"
                    >
                      C
                    </button>
                  </AppTooltip>
                  <AppTooltip text="Reset to 0 mV">
                    <button
                      type="button"
                      @click="ch1OffsetMv = 0"
                      :disabled="!ch1Enabled"
                      class="text-[9px] font-mono text-[var(--dim-color)] hover:text-[var(--fg-color)] px-1 border border-transparent hover:border-[var(--border-color)] cursor-pointer disabled:opacity-40"
                    >
                      0
                    </button>
                  </AppTooltip>
                </div>
              </div>
              <div class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)]">
                <button
                  type="button"
                  v-on="holdCh1OffsetDec"
                  :disabled="!ch1Enabled"
                  class="px-2 py-1 text-xs border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                >
                  -
                </button>
                <AppInput
                  type="text"
                  :model-value="ch1OffsetMv"
                  @update:model-value="val => ch1OffsetMv = parseInt(String(val || '0'), 10) || 0"
                  :disabled="!ch1Enabled"
                  class="text-center !bg-transparent !border-none !h-auto !py-1 tabular-nums"
                />
                <button
                  type="button"
                  v-on="holdCh1OffsetInc"
                  :disabled="!ch1Enabled"
                  class="px-2 py-1 text-xs border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                >
                  +
                </button>
              </div>
            </div>
          </div>

          <!-- Section 3: Real-Time Channel Metrics (4 Columns x 2 Rows) -->
          <div class="grid grid-cols-4 gap-1 pt-2 border-t border-[var(--osc-ch1)]/20 font-mono text-center">
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Vpp</span>
              <span class="text-[10px] font-bold tabular-nums truncate" :class="ch1Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch1Enabled ? formatVoltage(lastCapture?.ch1Metrics?.vPpMv ?? 0) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Vrms</span>
              <span class="text-[10px] tabular-nums truncate" :class="ch1Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch1Enabled ? formatVoltage(lastCapture?.ch1Metrics?.vRmsMv ?? 0) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Freq</span>
              <span class="text-[10px] font-bold tabular-nums truncate" :class="ch1Enabled ? 'text-[var(--status-ok-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch1Enabled ? formatFrequency(lastCapture?.ch1Metrics?.frequencyHz ?? null) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Duty</span>
              <span class="text-[10px] tabular-nums truncate" :class="ch1Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch1Enabled && lastCapture?.ch1Metrics?.dutyCyclePercent != null ? `${lastCapture.ch1Metrics.dutyCyclePercent.toFixed(1)}%` : '---' }}
              </span>
            </div>
          </div>
        </div>

        <!-- Channel 2 Controls Card -->
        <div class="border border-[var(--osc-ch2)]/40 bg-[var(--ui-bg-card)] p-2.5 flex flex-col gap-2">
          <!-- Section 1: State & Coupling Header -->
          <div class="flex items-center justify-between gap-1.5 border-b border-[var(--osc-ch2)]/30 pb-2">
            <div class="flex items-center gap-1.5 shrink-0">
              <AppToggle v-model="ch2Enabled" />
              <span class="text-xs font-bold uppercase text-[var(--osc-ch2)] tracking-wider">
                CH2
              </span>
            </div>

            <!-- Hardware Switches: 1X/10X + DC/AC/GND -->
            <div class="flex items-center gap-1 font-mono text-[10px]">
              <div class="flex items-center border border-[var(--border-color)]">
                <button
                  type="button"
                  @click="ch2ProbeAtten = '1'"
                  :disabled="!ch2Enabled"
                  class="px-1.5 py-0.5 text-center transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch2ProbeAtten === '1' ? 'bg-[var(--osc-ch2)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  1X
                </button>
                <button
                  type="button"
                  @click="ch2ProbeAtten = '10'"
                  :disabled="!ch2Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch2ProbeAtten === '10' ? 'bg-[var(--osc-ch2)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  10X
                </button>
              </div>

              <div class="flex items-center border border-[var(--border-color)]">
                <button
                  type="button"
                  @click="ch2Coupling = 'DC'"
                  :disabled="!ch2Enabled"
                  class="px-1.5 py-0.5 text-center transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch2Coupling === 'DC' ? 'bg-[var(--osc-ch2)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  DC
                </button>
                <button
                  type="button"
                  @click="ch2Coupling = 'AC'"
                  :disabled="!ch2Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch2Coupling === 'AC' ? 'bg-[var(--osc-ch2)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  AC
                </button>
                <button
                  type="button"
                  @click="ch2Coupling = 'GND'"
                  :disabled="!ch2Enabled"
                  class="px-1.5 py-0.5 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer disabled:opacity-40"
                  :class="ch2Coupling === 'GND' ? 'bg-[var(--osc-ch2)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  GND
                </button>
              </div>
            </div>
          </div>

          <!-- Section 2: Steppers (Volts/Div & Offset) -->
          <div class="grid grid-cols-2 gap-2 text-xs">
            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <label class="text-[9px] text-[var(--dim-color)] uppercase font-bold tracking-wider">Volts/Div</label>
                <AppTooltip text="Auto-scale V/div to fit signal">
                  <button
                    type="button"
                    @click="store.autoScaleChannel(2)"
                    :disabled="!ch2Enabled"
                    class="text-[9px] font-mono text-[var(--osc-ch2)]/80 hover:text-[var(--osc-ch2)] px-1 border border-transparent hover:border-[var(--osc-ch2)]/40 cursor-pointer disabled:opacity-40"
                  >
                    Fit
                  </button>
                </AppTooltip>
              </div>
              <div class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)]">
                <AppTooltip text="Decrease V/div">
                  <button
                    type="button"
                    v-on="holdCh2VDivDec"
                    :disabled="!ch2Enabled"
                    class="px-2 py-1 text-xs border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                  >
                    -
                  </button>
                </AppTooltip>
                <span class="w-full text-center py-1 text-xs font-mono tabular-nums text-[var(--fg-color)] select-none">
                  {{ formatVoltsPerDiv(ch2VoltsPerDiv) }}
                </span>
                <AppTooltip text="Increase V/div">
                  <button
                    type="button"
                    v-on="holdCh2VDivInc"
                    :disabled="!ch2Enabled"
                    class="px-2 py-1 text-xs border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                  >
                    +
                  </button>
                </AppTooltip>
              </div>
            </div>

            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <label class="text-[9px] text-[var(--dim-color)] uppercase font-bold tracking-wider">Offset</label>
                <div class="flex items-center gap-1">
                  <AppTooltip text="Auto-center curve on 0V reference line">
                    <button
                      type="button"
                      @click="store.autoCenterChannel(2)"
                      :disabled="!ch2Enabled"
                      class="text-[9px] font-mono text-[var(--osc-ch2)]/80 hover:text-[var(--osc-ch2)] px-1 border border-transparent hover:border-[var(--osc-ch2)]/40 cursor-pointer disabled:opacity-40"
                    >
                      C
                    </button>
                  </AppTooltip>
                  <AppTooltip text="Reset to 0 mV">
                    <button
                      type="button"
                      @click="ch2OffsetMv = 0"
                      :disabled="!ch2Enabled"
                      class="text-[9px] font-mono text-[var(--dim-color)] hover:text-[var(--fg-color)] px-1 border border-transparent hover:border-[var(--border-color)] cursor-pointer disabled:opacity-40"
                    >
                      0
                    </button>
                  </AppTooltip>
                </div>
              </div>
              <div class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)]">
                <button
                  type="button"
                  v-on="holdCh2OffsetDec"
                  :disabled="!ch2Enabled"
                  class="px-2 py-1 text-xs border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                >
                  -
                </button>
                <AppInput
                  type="text"
                  :model-value="ch2OffsetMv"
                  @update:model-value="val => ch2OffsetMv = parseInt(String(val || '0'), 10) || 0"
                  :disabled="!ch2Enabled"
                  class="text-center !bg-transparent !border-none !h-auto !py-1 tabular-nums"
                />
                <button
                  type="button"
                  v-on="holdCh2OffsetInc"
                  :disabled="!ch2Enabled"
                  class="px-2 py-1 text-xs border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer disabled:opacity-40"
                >
                  +
                </button>
              </div>
            </div>
          </div>

          <!-- Section 3: Real-Time Channel Metrics (4 Columns x 2 Rows) -->
          <div class="grid grid-cols-4 gap-1 pt-2 border-t border-[var(--osc-ch2)]/20 font-mono text-center">
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Vpp</span>
              <span class="text-[10px] font-bold tabular-nums truncate" :class="ch2Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch2Enabled ? formatVoltage(lastCapture?.ch2Metrics?.vPpMv ?? 0) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Vrms</span>
              <span class="text-[10px] tabular-nums truncate" :class="ch2Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch2Enabled ? formatVoltage(lastCapture?.ch2Metrics?.vRmsMv ?? 0) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Freq</span>
              <span class="text-[10px] font-bold tabular-nums truncate" :class="ch2Enabled ? 'text-[var(--status-ok-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch2Enabled ? formatFrequency(lastCapture?.ch2Metrics?.frequencyHz ?? null) : '---' }}
              </span>
            </div>
            <div class="flex flex-col">
              <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold">Duty</span>
              <span class="text-[10px] tabular-nums truncate" :class="ch2Enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)] opacity-60'">
                {{ ch2Enabled && lastCapture?.ch2Metrics?.dutyCyclePercent != null ? `${lastCapture.ch2Metrics.dutyCyclePercent.toFixed(1)}%` : '---' }}
              </span>
            </div>
          </div>
        </div>

        <!-- Trigger Controls -->
        <div class="border border-[var(--osc-trig)]/40 bg-[var(--ui-bg-card)] p-2.5 flex flex-col gap-2">
          <div class="flex items-center justify-between border-b border-[var(--osc-trig)]/30 pb-1.5">
            <span class="text-xs font-bold uppercase tracking-wider text-[var(--osc-trig)]">
              Trigger
            </span>
            <!-- Sweep Mode 3-Way Segmented Toggle -->
            <div class="flex items-center border border-[var(--osc-trig)]/30 font-mono text-[10px]">
              <button
                v-for="mode in ['Auto', 'Normal', 'Single']"
                :key="mode"
                type="button"
                @click="triggerSweep = mode"
                class="px-1.5 py-0.5 text-center transition-colors cursor-pointer"
                :class="triggerSweep === mode ? 'bg-[var(--osc-trig)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                {{ mode }}
              </button>
            </div>
          </div>

          <div class="grid grid-cols-2 gap-2 text-xs">
            <!-- Source & Slope Hardware Pills -->
            <div class="flex flex-col gap-1.5">
              <!-- Source -->
              <div class="flex items-center border border-[var(--border-color)] font-mono text-[10px]">
                <button
                  v-for="src in (activeScopeDescriptor?.triggerSources ?? ['CH1', 'CH2', 'EXT'])"
                  :key="src"
                  type="button"
                  @click="triggerSource = src"
                  class="flex-1 py-1 text-center transition-colors cursor-pointer"
                  :class="triggerSource === src ? 'bg-[var(--osc-trig)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  {{ src }}
                </button>
              </div>

              <!-- Slope -->
              <div class="flex items-center border border-[var(--border-color)] font-mono text-[10px]">
                <button
                  type="button"
                  @click="triggerSlope = 'Rising'"
                  class="flex-1 py-1 text-center transition-colors cursor-pointer"
                  :class="triggerSlope === 'Rising' ? 'bg-[var(--osc-trig)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  Rising ↗
                </button>
                <button
                  type="button"
                  @click="triggerSlope = 'Falling'"
                  class="flex-1 py-1 text-center border-l border-[var(--border-color)] transition-colors cursor-pointer"
                  :class="triggerSlope === 'Falling' ? 'bg-[var(--osc-trig)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  Falling ↘
                </button>
              </div>
            </div>

            <!-- Trigger Level Stepper -->
            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <label class="text-[9px] text-[var(--dim-color)] uppercase font-bold tracking-wider">Level</label>
                <AppTooltip text="Reset to 0 mV">
                  <button
                    type="button"
                    @click="triggerLevelMv = 0"
                    class="text-[9px] font-mono text-[var(--dim-color)] hover:text-[var(--fg-color)] px-1 border border-transparent hover:border-[var(--border-color)] cursor-pointer"
                  >
                    0
                  </button>
                </AppTooltip>
              </div>
              <div class="flex items-center border border-[var(--border-color)] bg-[var(--bg-color)]">
                <button
                  type="button"
                  v-on="holdTriggerLevelDec"
                  class="px-2 py-1 text-xs border-r border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
                >
                  -
                </button>
                <AppInput
                  type="text"
                  :model-value="triggerLevelMv"
                  @update:model-value="val => triggerLevelMv = parseInt(String(val || '0'), 10) || 0"
                  class="text-center !bg-transparent !border-none !h-auto !py-1 tabular-nums"
                />
                <button
                  type="button"
                  v-on="holdTriggerLevelInc"
                  class="px-2 py-1 text-xs border-l border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
                >
                  +
                </button>
              </div>
              <span class="text-[9px] font-mono text-center text-[var(--dim-color)]">mV</span>
            </div>
          </div>
        </div>

        <!-- Phase 2: Math Channel Card (When Active) -->
        <div v-if="mathEnabled" class="border border-[var(--osc-math)]/40 bg-[var(--ui-bg-card)] p-2.5 flex flex-col gap-2 font-mono">
          <div class="flex items-center justify-between border-b border-[var(--osc-math)]/30 pb-1.5">
            <span class="text-xs font-bold uppercase tracking-wider text-[var(--osc-math)]">
              Math Channel (M)
            </span>
            <button
              type="button"
              @click="mathEnabled = false"
              class="text-[9px] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer px-1"
            >
              ✕
            </button>
          </div>
          <div class="flex items-center border border-[var(--osc-math)]/30 text-[10px]">
            <button
              v-for="op in (['CH1-CH2', 'CH1+CH2', 'INV_CH1'] as const)"
              :key="op"
              type="button"
              @click="mathOp = op"
              class="flex-1 py-1 text-center transition-colors cursor-pointer"
              :class="mathOp === op ? 'bg-[var(--accent-color)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              {{ op }}
            </button>
          </div>
        </div>

        <!-- Phase 2: Extended Parametric Measurements Card -->
        <div v-if="showExtendedMetrics" class="border border-[var(--status-warn-border)] bg-[var(--surface-bg)] p-2.5 flex flex-col gap-2 font-mono">
          <div class="flex items-center justify-between border-b border-[var(--status-warn-border)] pb-1.5">
            <span class="text-xs font-bold uppercase tracking-wider text-[var(--status-warn-color)]">
              Parametric Analysis
            </span>
            <span class="text-[9px] text-[var(--dim-color)] uppercase">Lab DSP</span>
          </div>

          <div class="grid grid-cols-2 gap-2 text-[10px]">
            <div class="flex flex-col border border-[var(--border-color)] p-1.5 bg-[var(--bg-color)]/60">
              <span class="text-[8px] text-[var(--dim-color)] uppercase">V_top (90%)</span>
              <span class="text-[var(--fg-color)] font-bold tabular-nums">
                {{ extendedMetrics ? formatVoltage(extendedMetrics.vTopMv) : '---' }}
              </span>
            </div>
            <div class="flex flex-col border border-[var(--border-color)] p-1.5 bg-[var(--bg-color)]/60">
              <span class="text-[8px] text-[var(--dim-color)] uppercase">V_base (10%)</span>
              <span class="text-[var(--fg-color)] font-bold tabular-nums">
                {{ extendedMetrics ? formatVoltage(extendedMetrics.vBaseMv) : '---' }}
              </span>
            </div>
            <div class="flex flex-col border border-[var(--border-color)] p-1.5 bg-[var(--bg-color)]/60">
              <span class="text-[8px] text-[var(--dim-color)] uppercase">Rise Time (Tr)</span>
              <span class="text-[var(--status-ok-color)] font-bold tabular-nums">
                {{ extendedMetrics && extendedMetrics.riseTimeUs != null ? `${extendedMetrics.riseTimeUs.toFixed(2)} µs` : '---' }}
              </span>
            </div>
            <div class="flex flex-col border border-[var(--border-color)] p-1.5 bg-[var(--bg-color)]/60">
              <span class="text-[8px] text-[var(--dim-color)] uppercase">Fall Time (Tf)</span>
              <span class="text-[var(--status-ok-color)] font-bold tabular-nums">
                {{ extendedMetrics && extendedMetrics.fallTimeUs != null ? `${extendedMetrics.fallTimeUs.toFixed(2)} µs` : '---' }}
              </span>
            </div>
          </div>

          <div class="border border-[var(--border-color)] p-1.5 bg-[var(--bg-color)]/60 flex items-center justify-between text-[10px]">
            <span class="text-[8px] text-[var(--dim-color)] uppercase">Phase Diff (ΔΦ)</span>
            <span class="text-[var(--accent-color)] font-bold tabular-nums">
              {{ extendedMetrics && extendedMetrics.phaseDiffDeg != null ? `${extendedMetrics.phaseDiffDeg.toFixed(1)}°` : '---' }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- Agent Digest Drawer (Collapsible) -->
    <div
      v-if="showAgentDigest"
      class="border-t border-[var(--border-color)] bg-[var(--surface-bg)] p-3 max-h-60 overflow-y-auto font-mono text-xs text-[var(--fg-color)] flex flex-col gap-2 shrink-0"
    >
      <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-1">
        <span class="font-bold text-[var(--accent-color)] uppercase flex items-center gap-1.5">
          <Code class="w-3.5 h-3.5" />
          <span>LLM Agent Metrics Digest (Token-Optimized Scalar Stream)</span>
        </span>
        <div class="flex items-center gap-2">
          <button
            @click="copyAgentDigest"
            class="h-6 px-2 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] text-[var(--fg-color)] flex items-center gap-1 cursor-pointer transition-colors"
          >
            <Check v-if="copiedDigest" class="w-3 h-3 text-[var(--status-ok-color)]" />
            <Copy v-else class="w-3 h-3" />
            <span>{{ copiedDigest ? 'Copied' : 'Copy JSON' }}</span>
          </button>
          <button
            @click="showAgentDigest = false"
            class="ui-icon-btn !h-6 !w-6"
          >
            ✕
          </button>
        </div>
      </div>

      <pre class="whitespace-pre text-[11px] leading-tight text-[var(--fg-color)]">{{
        JSON.stringify(
          {
            ch1Metrics: lastCapture?.ch1Metrics,
            ch2Metrics: lastCapture?.ch2Metrics,
            timePerDivUs: lastCapture?.timePerDivUs,
            sampleRateHz: Number(lastCapture?.sampleRateHz ?? 0),
            samplesCount: lastCapture?.samplesCount,
            durationMs: Number(lastCapture?.durationMs ?? 0),
          },
          null,
          2
        )
      }}</pre>
    </div>

    <!-- Calibration & Probe Compensation Modal -->
    <div
      v-if="showCalibrationModal"
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
    >
      <div class="bg-[var(--bg-color)] border border-[var(--border-color)] w-full max-w-2xl max-h-[90vh] flex flex-col shadow-2xl text-[var(--fg-color)]">
        <!-- Modal Header -->
        <div class="h-9 ui-surface-title border-b border-[var(--border-color)] px-3 flex items-center justify-between shrink-0">
          <div class="flex items-center gap-2 text-xs">
            <Wrench class="w-3.5 h-3.5 text-[var(--status-warn-color)]" />
            <span class="font-bold tracking-wider uppercase text-[var(--fg-color)]">Oscilloscope Calibration</span>
          </div>
          <AppTooltip text="Close" position="bottom">
            <button
              type="button"
              class="ui-icon-btn !h-6 !w-6"
              :disabled="isCalibrating"
              @click="showCalibrationModal = false"
            >
              <X :size="14" />
            </button>
          </AppTooltip>
        </div>

        <!-- Mode Tabs -->
        <div class="flex border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 text-xs">
          <button
            @click="activeCalTab = 'probe'"
            class="px-4 py-2 border-b-2 font-bold cursor-pointer transition-colors"
            :class="activeCalTab === 'probe' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            Probe Compensation
          </button>
          <button
            @click="activeCalTab = 'offset'"
            class="px-4 py-2 border-b-2 font-bold cursor-pointer transition-colors"
            :class="activeCalTab === 'offset' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            Zero Baseline
          </button>
        </div>

        <!-- Modal Body -->
        <div class="p-4 flex flex-col gap-4 overflow-y-auto">
          <!-- Probe Compensation Tab -->
          <div v-if="activeCalTab === 'probe'" class="flex flex-col gap-3.5">
            <div class="p-3 bg-[var(--surface-bg)] border border-[var(--status-warn-border)] text-xs text-[var(--fg-color)] flex flex-col gap-1.5 leading-relaxed">
              <span class="font-bold text-[var(--status-warn-color)] uppercase tracking-wide">Procedure:</span>
              <p>1. Connect probe tip to the 1 kHz square wave calibration terminal (⎍ 1kHz 3.3V) and alligator clip to GND.</p>
              <p>2. Verify probe attenuation switch matches the selection below.</p>
              <p>3. Click <strong class="text-[var(--accent-color)]">Check Probe</strong> to sample the waveform and analyze compensation.</p>
              <p>4. If under/over-compensated, adjust the probe handle trimmer capacitor until waveform corners are flat.</p>
            </div>

            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-1">
                <label class="text-[10px] text-[var(--dim-color)] uppercase font-bold">Channel</label>
                <AppSelect v-model="calChannel" :options="calChannelOptions" :disabled="isCalibrating" />
              </div>
              <div class="flex flex-col gap-1">
                <label class="text-[10px] text-[var(--dim-color)] uppercase font-bold">Probe Attenuation</label>
                <AppSelect v-model="calProbeAtten" :options="calProbeOptions" :disabled="isCalibrating" />
              </div>
            </div>

            <div class="flex justify-end pt-2">
              <button
                @click="handleProbeCheck"
                :disabled="isCalibrating"
                class="h-6 px-3 text-[10px] font-bold uppercase border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] hover:opacity-90 transition-all flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
              >
                <Loader2 v-if="isCalibrating" class="w-3.5 h-3.5 animate-spin" />
                <RotateCw v-else class="w-3.5 h-3.5" />
                <span>{{ isCalibrating ? 'Acquiring Signal...' : 'Check Probe' }}</span>
              </button>
            </div>

            <!-- Error Banner -->
            <div v-if="calError" class="p-3 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-xs">
              <strong class="uppercase">[Calibration Error]:</strong> {{ calError }}
            </div>

            <!-- Results Card -->
            <div v-if="calResult && activeCalTab === 'probe'" class="border border-[var(--border-color)] bg-[var(--surface-bg)] p-3 flex flex-col gap-3">
              <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
                <span class="text-xs font-bold uppercase tracking-wider flex items-center gap-1.5">
                  <CheckCircle2 v-if="calResult.status === 'passed'" class="w-4 h-4 text-[var(--status-ok-color)]" />
                  <AlertTriangle v-else class="w-4 h-4 text-[var(--status-warn-color)]" />
                  <span>{{ calResult.status === 'passed' ? 'Calibration Passed' : 'Adjustment Needed' }}</span>
                </span>
                <span
                  class="text-[10px] px-2 py-0.5 border"
                  :class="calResult.status === 'passed'
                    ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold'
                    : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold'"
                >
                  {{ calResult.message }}
                </span>
              </div>

              <!-- Metrics -->
              <div class="grid grid-cols-3 gap-2 text-center text-xs">
                <div class="p-2 bg-[var(--bg-color)] border border-[var(--border-color)] flex flex-col">
                  <span class="text-[9px] text-[var(--dim-color)] uppercase font-bold">Frequency</span>
                  <span class="text-[var(--status-ok-color)] font-bold">{{ formatFrequency(calResult.measuredFreqHz) }}</span>
                  <span class="text-[8px] text-[var(--dim-color)]">Nominal: 1.00 kHz</span>
                </div>
                <div class="p-2 bg-[var(--bg-color)] border border-[var(--border-color)] flex flex-col">
                  <span class="text-[9px] text-[var(--dim-color)] uppercase font-bold">Peak-to-Peak (Vpp)</span>
                  <span class="text-[var(--fg-color)] font-bold">{{ formatVoltage(calResult.measuredVppMv ?? 0) }}</span>
                  <span class="text-[8px] text-[var(--dim-color)]">Nominal: ~3.3V</span>
                </div>
                <div class="p-2 bg-[var(--bg-color)] border border-[var(--border-color)] flex flex-col">
                  <span class="text-[9px] text-[var(--dim-color)] uppercase font-bold">Duty Cycle</span>
                  <span class="text-[var(--fg-color)] font-bold">
                    {{ calResult.measuredDutyCyclePercent != null ? `${calResult.measuredDutyCyclePercent.toFixed(1)}%` : '---' }}
                  </span>
                  <span class="text-[8px] text-[var(--dim-color)]">Nominal: 50.0%</span>
                </div>
              </div>

              <!-- Advice -->
              <div v-if="calResult.probeTrimAdvice" class="p-2.5 bg-[var(--bg-color)] border border-[var(--border-color)] text-xs text-[var(--fg-color)]">
                <span class="font-bold text-[var(--accent-color)] uppercase text-[10px] block mb-1">Probe Trimmer Recommendation:</span>
                <p>{{ calResult.probeTrimAdvice }}</p>
              </div>
            </div>
          </div>

          <!-- Zero Baseline Offset Tab -->
          <div v-else-if="activeCalTab === 'offset'" class="flex flex-col gap-3.5">
            <div class="p-3 bg-[var(--ui-bg-card)] border border-[var(--status-info-border)] text-xs text-[var(--fg-color)] flex flex-col gap-1.5 leading-relaxed">
              <span class="font-bold text-[var(--status-info-color)] uppercase tracking-wide">Procedure:</span>
              <p>1. Disconnect probes from all test circuits or set channel coupling to GND.</p>
              <p>2. Click <strong class="text-[var(--fg-color)]">Calibrate Zero</strong> to measure and align analog ADC zero offsets across PGA gain stages.</p>
            </div>

            <div class="flex justify-end pt-2">
              <button
                @click="handleZeroOffsetCalibration"
                :disabled="isCalibrating"
                class="px-4 py-2 text-xs font-bold uppercase border border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)] hover:bg-[var(--status-info-color)] hover:text-[var(--bg-color)] transition-all flex items-center gap-2 cursor-pointer disabled:opacity-50"
              >
                <Loader2 v-if="isCalibrating" class="w-3.5 h-3.5 animate-spin" />
                <RotateCw v-else class="w-3.5 h-3.5" />
                <span>{{ isCalibrating ? 'Calibrating...' : 'Calibrate Zero' }}</span>
              </button>
            </div>

            <!-- Error Banner -->
            <div v-if="calError" class="p-3 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-xs">
              <strong class="uppercase">[Calibration Error]:</strong> {{ calError }}
            </div>

            <!-- Zero Results Card -->
            <div v-if="calResult && activeCalTab === 'offset'" class="border border-[var(--border-color)] bg-[var(--ui-bg-card)] p-3 flex flex-col gap-3">
              <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
                <span class="text-xs font-bold uppercase tracking-wider flex items-center gap-1.5">
                  <CheckCircle2 class="w-4 h-4 text-[var(--status-ok-color)]" />
                  <span>{{ calResult.message }}</span>
                </span>
                <span class="text-[10px] px-2 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold">
                  {{ calResult.status.toUpperCase() }}
                </span>
              </div>

              <div v-if="calResult.zeroOffsetsMv" class="flex flex-col gap-1.5 text-xs">
                <span class="text-[10px] text-[var(--dim-color)] uppercase font-bold">Hardware Zero Offsets Across Ranges:</span>
                <div class="flex flex-wrap gap-1.5">
                  <span
                    v-for="(val, idx) in calResult.zeroOffsetsMv"
                    :key="`range-${idx}`"
                    class="px-2 py-1 bg-[var(--ui-bg-base)] border border-[var(--border-color)] text-[10px] font-mono text-[var(--fg-color)]"
                  >
                    R{{ idx }}: {{ val }}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Modal Footer -->
        <div class="border-t border-[var(--border-color)] px-3 py-2 flex items-center justify-between bg-[var(--ui-bg-card)]/30 text-xs">
          <span class="text-[10px] text-[var(--dim-color)]">{{ activeScopeDescriptor?.name || 'Oscilloscope' }} Hardware Calibration</span>
          <button
            type="button"
            class="h-6 px-3 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] font-bold uppercase text-[var(--fg-color)] transition-colors cursor-pointer"
            @click="showCalibrationModal = false"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
