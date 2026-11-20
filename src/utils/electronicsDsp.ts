/**
 * Digital Signal Processing (DSP) & Waveform Math Utilities
 * for ONE-SHIP Electronics Workstation.
 *
 * Provides:
 * - Point-by-point math channel operations (CH1-CH2, CH1+CH2, INV_CH1)
 * - Radix-2 Cooley-Tukey FFT with Hanning windowing for spectrum analysis
 * - Extended parametric metrics (Rise/Fall time, Top/Base, Phase Difference)
 * - CSV and JSON waveform export utilities
 */

export interface FftSpectrumResult {
  freqsHz: number[];
  magnitudesDb: number[];
  peakFreqHz: number;
  peakMagnitudeDb: number;
  nyquistHz: number;
}

export interface ExtendedWaveformMetrics {
  vTopMv: number;
  vBaseMv: number;
  riseTimeUs: number | null;
  fallTimeUs: number | null;
  phaseDiffDeg: number | null;
}

/**
 * Computes vector math traces between oscilloscope channels.
 */
export function computeMathTrace(
  ch1: number[],
  ch2: number[],
  op: 'CH1-CH2' | 'CH1+CH2' | 'INV_CH1'
): number[] {
  if (op === 'INV_CH1') {
    return ch1.map((v) => -v);
  }

  const length = Math.min(ch1.length, ch2.length);
  const result = new Array<number>(length);

  if (op === 'CH1-CH2') {
    for (let i = 0; i < length; i++) {
      result[i] = ch1[i] - ch2[i];
    }
  } else if (op === 'CH1+CH2') {
    for (let i = 0; i < length; i++) {
      result[i] = ch1[i] + ch2[i];
    }
  }

  return result;
}

/**
 * In-place Radix-2 Cooley-Tukey Fast Fourier Transform (FFT).
 * Real and imaginary arrays must have length equal to a power of 2.
 */
function radix2Fft(real: Float64Array, imag: Float64Array): void {
  const n = real.length;
  // Bit-reversal permutation
  let j = 0;
  for (let i = 0; i < n - 1; i++) {
    if (i < j) {
      const tempR = real[i];
      real[i] = real[j];
      real[j] = tempR;

      const tempI = imag[i];
      imag[i] = imag[j];
      imag[j] = tempI;
    }
    let k = n >> 1;
    while (k <= j) {
      j -= k;
      k >>= 1;
    }
    j += k;
  }

  // Cooley-Tukey butterflies
  for (let len = 2; len <= n; len <<= 1) {
    const halfLen = len >> 1;
    const angleStep = (-2 * Math.PI) / len;
    const wStepR = Math.cos(angleStep);
    const wStepI = Math.sin(angleStep);

    for (let i = 0; i < n; i += len) {
      let wR = 1.0;
      let wI = 0.0;
      for (let k = 0; k < halfLen; k++) {
        const uR = real[i + k];
        const uI = imag[i + k];
        const vR = real[i + k + halfLen] * wR - imag[i + k + halfLen] * wI;
        const vI = real[i + k + halfLen] * wI + imag[i + k + halfLen] * wR;

        real[i + k] = uR + vR;
        imag[i + k] = uI + vI;
        real[i + k + halfLen] = uR - vR;
        imag[i + k + halfLen] = uI - vI;

        const nextWR = wR * wStepR - wI * wStepI;
        wI = wR * wStepI + wI * wStepR;
        wR = nextWR;
      }
    }
  }
}

/**
 * Computes frequency spectrum (dBV vs Hz) using 1024/2048 point FFT
 * with Hanning windowing to prevent spectral leakage.
 */
export function computeFftSpectrum(
  samples: number[],
  sampleRateHz: number,
  targetFftSize: number = 1024
): FftSpectrumResult | null {
  if (samples.length < 64 || sampleRateHz <= 0) {
    return null;
  }

  // Find highest power of 2 <= targetFftSize and <= samples.length
  let n = 1;
  while (n * 2 <= Math.min(targetFftSize, samples.length)) {
    n *= 2;
  }
  if (n < 64) return null;

  const real = new Float64Array(n);
  const imag = new Float64Array(n);

  // Compute DC mean of the selected window to avoid 0Hz spike dominating spectrum
  let sum = 0;
  for (let i = 0; i < n; i++) {
    sum += samples[i];
  }
  const mean = sum / n;

  // Apply Hanning Window: w[i] = 0.5 * (1 - cos(2*pi*i / (n - 1)))
  // and convert mV to Volts
  const twoPiOverN = (2 * Math.PI) / (n - 1);
  for (let i = 0; i < n; i++) {
    const window = 0.5 * (1 - Math.cos(i * twoPiOverN));
    real[i] = ((samples[i] - mean) / 1000.0) * window;
    imag[i] = 0.0;
  }

  radix2Fft(real, imag);

  const halfN = n / 2;
  const freqsHz = new Array<number>(halfN);
  const magnitudesDb = new Array<number>(halfN);
  const freqStep = sampleRateHz / n;
  const nyquistHz = sampleRateHz / 2;

  let peakFreqHz = 0;
  let peakMagnitudeDb = -Infinity;

  // Window coherent gain compensation for Hanning is 2.0 (factor 4 in power / 2 in amplitude)
  const normFactor = 2.0 / halfN;

  for (let i = 0; i < halfN; i++) {
    freqsHz[i] = i * freqStep;
    const magLinear = Math.sqrt(real[i] * real[i] + imag[i] * imag[i]) * normFactor;
    // Log scale dBV = 20 * log10(Vrms / 1V)
    const db = 20 * Math.log10(Math.max(magLinear, 1e-6));
    magnitudesDb[i] = Math.max(db, -100.0); // Floor at -100 dBV

    // Ignore immediate 0-index DC bin for peak detection
    if (i > 1 && db > peakMagnitudeDb) {
      peakMagnitudeDb = db;
      peakFreqHz = freqsHz[i];
    }
  }

  return {
    freqsHz,
    magnitudesDb,
    peakFreqHz,
    peakMagnitudeDb: peakMagnitudeDb === -Infinity ? -100 : peakMagnitudeDb,
    nyquistHz,
  };
}

/**
 * Computes extended parametric measurements:
 * - V_top, V_base (histogram 10%-90% levels)
 * - Rise Time (t_10% -> 90%)
 * - Fall Time (t_90% -> 10%)
 * - CH1 vs CH2 Phase difference in degrees
 */
export function computeExtendedMetrics(
  samples: number[],
  timePerSampleUs: number,
  ch2Samples?: number[]
): ExtendedWaveformMetrics {
  if (samples.length < 20 || timePerSampleUs <= 0) {
    return {
      vTopMv: 0,
      vBaseMv: 0,
      riseTimeUs: null,
      fallTimeUs: null,
      phaseDiffDeg: null,
    };
  }

  // Sort copy to find 10% and 90% percentiles (V_base and V_top)
  const sorted = [...samples].sort((a, b) => a - b);
  const idx10 = Math.floor(sorted.length * 0.1);
  const idx90 = Math.floor(sorted.length * 0.9);
  const vBaseMv = sorted[idx10];
  const vTopMv = sorted[idx90];
  const vAmp = vTopMv - vBaseMv;

  let riseTimeUs: number | null = null;
  let fallTimeUs: number | null = null;

  if (vAmp > 50) {
    const thresh10 = vBaseMv + 0.1 * vAmp;
    const thresh90 = vBaseMv + 0.9 * vAmp;

    const riseTimes: number[] = [];
    const fallTimes: number[] = [];

    let edgeStartIdx: number | null = null;
    let edgeRising: boolean | null = null;

    for (let i = 1; i < samples.length; i++) {
      const prev = samples[i - 1];
      const curr = samples[i];

      // Detect rising edge entry
      if (prev <= thresh10 && curr > thresh10 && edgeStartIdx === null) {
        edgeStartIdx = i;
        edgeRising = true;
      } else if (edgeRising === true && curr >= thresh90 && edgeStartIdx !== null) {
        const deltaSamples = i - edgeStartIdx;
        riseTimes.push(deltaSamples * timePerSampleUs);
        edgeStartIdx = null;
        edgeRising = null;
      }

      // Detect falling edge entry
      if (prev >= thresh90 && curr < thresh90 && edgeStartIdx === null) {
        edgeStartIdx = i;
        edgeRising = false;
      } else if (edgeRising === false && curr <= thresh10 && edgeStartIdx !== null) {
        const deltaSamples = i - edgeStartIdx;
        fallTimes.push(deltaSamples * timePerSampleUs);
        edgeStartIdx = null;
        edgeRising = null;
      }
    }

    if (riseTimes.length > 0) {
      riseTimeUs = riseTimes.reduce((acc, v) => acc + v, 0) / riseTimes.length;
    }
    if (fallTimes.length > 0) {
      fallTimeUs = fallTimes.reduce((acc, v) => acc + v, 0) / fallTimes.length;
    }
  }

  // Compute Phase Difference between CH1 and CH2 if CH2 is supplied
  let phaseDiffDeg: number | null = null;
  if (ch2Samples && ch2Samples.length === samples.length) {
    const mid1 = (Math.max(...samples) + Math.min(...samples)) / 2;
    const mid2 = (Math.max(...ch2Samples) + Math.min(...ch2Samples)) / 2;

    // Find first rising zero-crossing for both channels
    let cross1: number | null = null;
    let cross2: number | null = null;
    let period1Samples: number | null = null;

    for (let i = 1; i < samples.length; i++) {
      if (samples[i - 1] < mid1 && samples[i] >= mid1) {
        if (cross1 === null) {
          cross1 = i;
        } else if (period1Samples === null) {
          period1Samples = i - cross1;
          break;
        }
      }
    }

    for (let i = 1; i < ch2Samples.length; i++) {
      if (ch2Samples[i - 1] < mid2 && ch2Samples[i] >= mid2) {
        cross2 = i;
        break;
      }
    }

    if (cross1 !== null && cross2 !== null && period1Samples && period1Samples > 0) {
      const deltaSamples = cross2 - cross1;
      const deg = ((deltaSamples / period1Samples) * 360.0) % 360.0;
      phaseDiffDeg = deg > 180 ? deg - 360 : deg < -180 ? deg + 360 : deg;
    }
  }

  return {
    vTopMv,
    vBaseMv,
    riseTimeUs,
    fallTimeUs,
    phaseDiffDeg,
  };
}

/**
 * Triggers a browser file download for exported waveform data.
 */
function downloadFile(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Exports oscilloscope capture points to CSV format.
 */
export function exportCaptureCsv(
  ch1Samples: number[],
  ch2Samples: number[],
  timePerSampleUs: number
): void {
  const total = Math.max(ch1Samples.length, ch2Samples.length);
  const rows: string[] = ['index,time_us,ch1_mv,ch2_mv,math_diff_mv'];

  for (let i = 0; i < total; i++) {
    const t = (i * timePerSampleUs).toFixed(4);
    const v1 = i < ch1Samples.length ? ch1Samples[i].toFixed(2) : '';
    const v2 = i < ch2Samples.length ? ch2Samples[i].toFixed(2) : '';
    const diff =
      i < ch1Samples.length && i < ch2Samples.length
        ? (ch1Samples[i] - ch2Samples[i]).toFixed(2)
        : '';
    rows.push(`${i},${t},${v1},${v2},${diff}`);
  }

  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  downloadFile(rows.join('\n'), `oneship_scope_${timestamp}.csv`, 'text/csv');
}

/**
 * Exports full capture session and metrics to JSON format.
 */
export function exportCaptureJson(captureData: unknown): void {
  const jsonStr = JSON.stringify(captureData, null, 2);
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  downloadFile(jsonStr, `oneship_scope_${timestamp}.json`, 'application/json');
}
