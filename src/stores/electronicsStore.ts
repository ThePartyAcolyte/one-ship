import { defineStore } from 'pinia';
import { ref, watch, computed } from 'vue';
import { daemonCall } from '../utils/daemonClient';
import type {
  OscilloscopeCaptureResult,
  OscilloscopeChannelConfig,
  OscilloscopeTriggerConfig,
  DiscoveredInstrument,
  OscilloscopeDescriptor,
} from '../types';

export function formatVoltsPerDivLabel(mv: number): string {
  return mv < 1000 ? `${mv} mV / div` : `${mv / 1000} V / div`;
}

export function formatTimebaseLabel(us: number): string {
  if (us < 1) return `${Math.round(us * 1000)} ns / div`;
  if (us < 1000) return `${us} µs / div`;
  return `${us / 1000} ms / div`;
}

export const DEFAULT_VOLTS_PER_DIV_MV = [5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];
export const DEFAULT_TIMEBASE_STEPS_US = [
  0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000,
];

export const VOLTS_PER_DIV_OPTIONS = DEFAULT_VOLTS_PER_DIV_MV.map((mv) => ({
  label: formatVoltsPerDivLabel(mv),
  value: String(mv),
}));

export const TIMEBASE_OPTIONS = DEFAULT_TIMEBASE_STEPS_US.map((us) => ({
  label: formatTimebaseLabel(us),
  value: String(us),
}));

const STORAGE_KEY = 'oneship_oscilloscope_config';

interface PersistedOscilloscopeConfig {
  ch1Enabled: boolean;
  ch1VoltsPerDiv: string;
  ch1OffsetMv: number;
  ch1Coupling: string;
  ch1ProbeAtten: string;
  ch2Enabled: boolean;
  ch2VoltsPerDiv: string;
  ch2OffsetMv: number;
  ch2Coupling: string;
  ch2ProbeAtten: string;
  timebaseDivUs: string;
  triggerSource: string;
  triggerSlope: string;
  triggerSweep: string;
  triggerLevelMv: number;
  showTimeCursors: boolean;
  showVoltCursors: boolean;
  cursorTime1Us: number;
  cursorTime2Us: number;
  cursorVolt1Mv: number;
  cursorVolt2Mv: number;
  displayMode: 'overlay' | 'split';
  mathEnabled: boolean;
  mathOp: 'CH1-CH2' | 'CH1+CH2' | 'INV_CH1';
  showFft: boolean;
  showExtendedMetrics: boolean;
}

function loadInitialConfig(): PersistedOscilloscopeConfig {
  const defaults: PersistedOscilloscopeConfig = {
    ch1Enabled: true,
    ch1VoltsPerDiv: '1000',
    ch1OffsetMv: 0,
    ch1Coupling: 'DC',
    ch1ProbeAtten: '1',
    ch2Enabled: true,
    ch2VoltsPerDiv: '1000',
    ch2OffsetMv: 0,
    ch2Coupling: 'DC',
    ch2ProbeAtten: '1',
    timebaseDivUs: '200',
    triggerSource: 'CH1',
    triggerSlope: 'Rising',
    triggerSweep: 'Auto',
    triggerLevelMv: 0,
    showTimeCursors: false,
    showVoltCursors: false,
    cursorTime1Us: -400,
    cursorTime2Us: 400,
    cursorVolt1Mv: 1000,
    cursorVolt2Mv: -1000,
    displayMode: 'overlay',
    mathEnabled: false,
    mathOp: 'CH1-CH2',
    showFft: false,
    showExtendedMetrics: false,
  };

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<PersistedOscilloscopeConfig>;
    return { ...defaults, ...parsed };
  } catch {
    return defaults;
  }
}

export const useElectronicsStore = defineStore('electronics', () => {
  const initial = loadInitialConfig();

  // Hardware & Capability Manifest State
  const isScanning = ref<boolean>(false);
  const connectedOwon = ref<DiscoveredInstrument | null>(null);
  const supportedOscilloscopes = ref<OscilloscopeDescriptor[]>([]);
  const selectedScopeId = ref<string>('owon_vds1022');
  const activeScopeDescriptor = computed<OscilloscopeDescriptor | null>(() => {
    return (
      supportedOscilloscopes.value.find((s) => s.id === selectedScopeId.value) ??
      supportedOscilloscopes.value[0] ??
      null
    );
  });

  // Acquisition State
  const isCapturing = ref<boolean>(false);
  const isContinuousRunning = ref<boolean>(false);
  const isAutoSetting = ref<boolean>(false);
  const lastCapture = ref<OscilloscopeCaptureResult | null>(null);
  const captureError = ref<string | null>(null);
  let captureLoopTimer: number | null = null;

  // Channel 1 Settings
  const ch1Enabled = ref<boolean>(initial.ch1Enabled);
  const ch1VoltsPerDiv = ref<string>(initial.ch1VoltsPerDiv);
  const ch1OffsetMv = ref<number>(initial.ch1OffsetMv);
  const ch1Coupling = ref<string>(initial.ch1Coupling);
  const ch1ProbeAtten = ref<string>(initial.ch1ProbeAtten);

  // Channel 2 Settings
  const ch2Enabled = ref<boolean>(initial.ch2Enabled);
  const ch2VoltsPerDiv = ref<string>(initial.ch2VoltsPerDiv);
  const ch2OffsetMv = ref<number>(initial.ch2OffsetMv);
  const ch2Coupling = ref<string>(initial.ch2Coupling);
  const ch2ProbeAtten = ref<string>(initial.ch2ProbeAtten);

  // Timebase & Trigger
  const timebaseDivUs = ref<string>(initial.timebaseDivUs);
  const triggerSource = ref<string>(initial.triggerSource);
  const triggerSlope = ref<string>(initial.triggerSlope);
  const triggerSweep = ref<string>(initial.triggerSweep);
  const triggerLevelMv = ref<number>(initial.triggerLevelMv);

  // Cursors State
  const showTimeCursors = ref<boolean>(initial.showTimeCursors);
  const showVoltCursors = ref<boolean>(initial.showVoltCursors);
  const cursorTime1Us = ref<number>(initial.cursorTime1Us);
  const cursorTime2Us = ref<number>(initial.cursorTime2Us);
  const cursorVolt1Mv = ref<number>(initial.cursorVolt1Mv);
  const cursorVolt2Mv = ref<number>(initial.cursorVolt2Mv);

  // Visual Display Mode: 'overlay' (Single Grid) or 'split' (Dual Grid)
  const displayMode = ref<'overlay' | 'split'>(initial.displayMode);

  // Phase 2 Lab Tools State: Math Channel, FFT, Extended Metrics
  const mathEnabled = ref<boolean>(initial.mathEnabled);
  const mathOp = ref<'CH1-CH2' | 'CH1+CH2' | 'INV_CH1'>(initial.mathOp);
  const showFft = ref<boolean>(initial.showFft);
  const showExtendedMetrics = ref<boolean>(initial.showExtendedMetrics);

  // Calibration summary cache
  const lastCalSummary = ref<string | null>(null);

  // Persist configuration to localStorage
  function persistConfig() {
    const config: PersistedOscilloscopeConfig = {
      ch1Enabled: ch1Enabled.value,
      ch1VoltsPerDiv: ch1VoltsPerDiv.value,
      ch1OffsetMv: ch1OffsetMv.value,
      ch1Coupling: ch1Coupling.value,
      ch1ProbeAtten: ch1ProbeAtten.value,
      ch2Enabled: ch2Enabled.value,
      ch2VoltsPerDiv: ch2VoltsPerDiv.value,
      ch2OffsetMv: ch2OffsetMv.value,
      ch2Coupling: ch2Coupling.value,
      ch2ProbeAtten: ch2ProbeAtten.value,
      timebaseDivUs: timebaseDivUs.value,
      triggerSource: triggerSource.value,
      triggerSlope: triggerSlope.value,
      triggerSweep: triggerSweep.value,
      triggerLevelMv: triggerLevelMv.value,
      showTimeCursors: showTimeCursors.value,
      showVoltCursors: showVoltCursors.value,
      cursorTime1Us: cursorTime1Us.value,
      cursorTime2Us: cursorTime2Us.value,
      cursorVolt1Mv: cursorVolt1Mv.value,
      cursorVolt2Mv: cursorVolt2Mv.value,
      displayMode: displayMode.value,
      mathEnabled: mathEnabled.value,
      mathOp: mathOp.value,
      showFft: showFft.value,
      showExtendedMetrics: showExtendedMetrics.value,
    };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
    } catch {
      // Ignore quota errors in private mode
    }
  }

  watch(
    () => [
      ch1Enabled.value, ch1VoltsPerDiv.value, ch1OffsetMv.value, ch1Coupling.value, ch1ProbeAtten.value,
      ch2Enabled.value, ch2VoltsPerDiv.value, ch2OffsetMv.value, ch2Coupling.value, ch2ProbeAtten.value,
      timebaseDivUs.value, triggerSource.value, triggerSlope.value, triggerSweep.value, triggerLevelMv.value,
      showTimeCursors.value, showVoltCursors.value, cursorTime1Us.value, cursorTime2Us.value,
      cursorVolt1Mv.value, cursorVolt2Mv.value, displayMode.value, mathEnabled.value, mathOp.value,
      showFft.value, showExtendedMetrics.value,
    ],
    () => persistConfig()
  );

  async function scanHardware() {
    isScanning.value = true;
    try {
      const [list, descriptors] = await Promise.all([
        daemonCall('scan_connected_instruments', {}),
        daemonCall('list_supported_oscilloscopes', {}),
      ]);
      supportedOscilloscopes.value = descriptors;
      connectedOwon.value =
        list.find((i: DiscoveredInstrument) => i.id === 'owon_vds1022' || i.id === 'owon-vds1022') ?? null;
      if (connectedOwon.value) {
        selectedScopeId.value = connectedOwon.value.id;
      }
    } catch (err: unknown) {
      console.error('Failed to scan instruments:', err);
    } finally {
      isScanning.value = false;
    }
  }

  async function performCapture(): Promise<boolean> {
    if (isCapturing.value) return false;
    isCapturing.value = true;
    captureError.value = null;

    const ch1Config: OscilloscopeChannelConfig = {
      enabled: ch1Enabled.value,
      voltsPerDivMv: parseFloat(ch1VoltsPerDiv.value),
      verticalOffsetMv: ch1OffsetMv.value,
      coupling: ch1Coupling.value,
      probeAttenuation: parseInt(ch1ProbeAtten.value, 10),
    };

    const ch2Config: OscilloscopeChannelConfig = {
      enabled: ch2Enabled.value,
      voltsPerDivMv: parseFloat(ch2VoltsPerDiv.value),
      verticalOffsetMv: ch2OffsetMv.value,
      coupling: ch2Coupling.value,
      probeAttenuation: parseInt(ch2ProbeAtten.value, 10),
    };

    const triggerConfig: OscilloscopeTriggerConfig = {
      source: triggerSource.value,
      slope: triggerSlope.value,
      sweepMode: triggerSweep.value,
      levelMv: triggerLevelMv.value,
    };

    try {
      const res = await daemonCall('capture_oscilloscope', {
        ch1: ch1Config,
        ch2: ch2Config,
        trigger: triggerConfig,
        timebaseDivUs: parseFloat(timebaseDivUs.value),
      });
      lastCapture.value = res;
      return true;
    } catch (err: unknown) {
      captureError.value = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      isCapturing.value = false;
    }
  }

  async function runContinuousLoop() {
    if (!isContinuousRunning.value) return;
    await performCapture();
    if (isContinuousRunning.value) {
      captureLoopTimer = window.setTimeout(() => {
        runContinuousLoop();
      }, 150);
    }
  }

  function startContinuous() {
    if (isContinuousRunning.value) return;
    isContinuousRunning.value = true;
    runContinuousLoop();
  }

  function stopContinuous() {
    isContinuousRunning.value = false;
    if (captureLoopTimer !== null) {
      window.clearTimeout(captureLoopTimer);
      captureLoopTimer = null;
    }
  }

  async function triggerSingleCapture() {
    stopContinuous();
    await performCapture();
  }

  function stepVoltsPerDiv(ch: 1 | 2, direction: -1 | 1) {
    const targetRef = ch === 1 ? ch1VoltsPerDiv : ch2VoltsPerDiv;
    const options = activeScopeDescriptor.value?.voltsPerDivOptionsMv.map(String) ??
      VOLTS_PER_DIV_OPTIONS.map((opt) => opt.value);
    const currentIndex = options.findIndex((val) => val === targetRef.value);
    if (currentIndex === -1) return;
    const nextIndex = Math.max(0, Math.min(options.length - 1, currentIndex + direction));
    targetRef.value = options[nextIndex];
  }

  function stepTimebase(direction: -1 | 1) {
    const options = activeScopeDescriptor.value?.timebaseStepsUs.map(String) ??
      TIMEBASE_OPTIONS.map((opt) => opt.value);
    const currentIndex = options.findIndex((val) => val === timebaseDivUs.value);
    if (currentIndex === -1) return;
    const nextIndex = Math.max(0, Math.min(options.length - 1, currentIndex + direction));
    timebaseDivUs.value = options[nextIndex];
  }

  // Auto-centers the vertical offset of the chosen channel onto the zero grid line.
  function autoCenterChannel(ch: 1 | 2) {
    const capture = lastCapture.value;
    if (!capture) return;
    const metrics = ch === 1 ? capture.ch1Metrics : capture.ch2Metrics;
    if (!metrics) return;
    const midpoint = metrics.vAvgMv !== 0 ? metrics.vAvgMv : (metrics.vMaxMv + metrics.vMinMv) / 2;
    const targetOffset = -Math.round(midpoint);
    if (ch === 1) ch1OffsetMv.value = targetOffset;
    else ch2OffsetMv.value = targetOffset;
  }

  // Auto-scales vertical V/div for the chosen channel (occupies 60-75% screen height).
  function autoScaleChannel(ch: 1 | 2) {
    const capture = lastCapture.value;
    if (!capture) return;
    const metrics = ch === 1 ? capture.ch1Metrics : capture.ch2Metrics;
    if (!metrics || metrics.vPpMv <= 0) return;
    const targetRef = ch === 1 ? ch1VoltsPerDiv : ch2VoltsPerDiv;
    const requiredMvPerDiv = metrics.vPpMv / 6;
    const bestOpt = VOLTS_PER_DIV_OPTIONS.find((opt) => parseFloat(opt.value) >= requiredMvPerDiv);
    targetRef.value = bestOpt ? bestOpt.value : '5000';
  }

  /**
   * Auto-adjusts horizontal timebase according to measured signal frequency
   * to display 2 to 4 full cycles across the 10 screen divisions.
   */
  function autoTimebase() {
    const capture = lastCapture.value;
    if (!capture) return;

    const freqHz =
      triggerSource.value === 'CH2'
        ? (capture.ch2Metrics.frequencyHz ?? capture.ch1Metrics.frequencyHz)
        : (capture.ch1Metrics.frequencyHz ?? capture.ch2Metrics.frequencyHz);

    if (!freqHz || freqHz <= 0) return;

    // Period in microseconds
    const periodUs = 1000000 / freqHz;
    // Aim for 3 cycles across 10 divs: divUs = (3 * periodUs) / 10 = 0.3 * periodUs
    const idealDivUs = 0.3 * periodUs;

    // Find closest standard timebase option on logarithmic scale
    let bestOption: (typeof TIMEBASE_OPTIONS)[number] = TIMEBASE_OPTIONS[0];
    let minDiff = Infinity;

    for (const opt of TIMEBASE_OPTIONS) {
      const optUs = parseFloat(opt.value);
      const diff = Math.abs(Math.log(optUs) - Math.log(idealDivUs));
      if (diff < minDiff) {
        minDiff = diff;
        bestOption = opt;
      }
    }

    timebaseDivUs.value = bestOption.value;
  }

  /**
   * Comprehensive laboratory AUTO SET:
   * Captures initial frame if needed, auto-scales voltage and centers offset
   * on active channels, sets timebase to frame 2-3 cycles, and sets trigger level to signal midpoint.
   */
  async function autoSetOscilloscope(): Promise<void> {
    isAutoSetting.value = true;
    try {
      if (!lastCapture.value) {
        await performCapture();
      }
      if (!lastCapture.value) return;

      if (ch1Enabled.value) {
        autoScaleChannel(1);
        autoCenterChannel(1);
      }
      if (ch2Enabled.value) {
        autoScaleChannel(2);
        autoCenterChannel(2);
      }

      autoTimebase();

      const capture = lastCapture.value;
      const trgMetrics = triggerSource.value === 'CH2' ? capture.ch2Metrics : capture.ch1Metrics;
      if (trgMetrics) {
        triggerLevelMv.value = Math.round(trgMetrics.vAvgMv);
        triggerSweep.value = 'Auto';
      }

      if (!isContinuousRunning.value) {
        await performCapture();
      }
    } finally {
      isAutoSetting.value = false;
    }
  }

  function toggleDisplayMode() {
    displayMode.value = displayMode.value === 'overlay' ? 'split' : 'overlay';
  }

  return {
    // Hardware & Manifest State
    isScanning,
    connectedOwon,
    supportedOscilloscopes,
    selectedScopeId,
    activeScopeDescriptor,
    scanHardware,

    // Acquisition & Channel State
    isCapturing,
    isContinuousRunning,
    isAutoSetting,
    lastCapture,
    captureError,
    performCapture,
    startContinuous,
    stopContinuous,
    triggerSingleCapture,
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

    // Timebase, Trigger & Cursors
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

    // Display & Actions
    displayMode,
    lastCalSummary,
    toggleDisplayMode,
    mathEnabled,
    mathOp,
    showFft,
    showExtendedMetrics,
    stepVoltsPerDiv,
    stepTimebase,
    autoCenterChannel,
    autoScaleChannel,
    autoTimebase,
    autoSetOscilloscope,
  };
});
