export type QuotaCategory = 'gemini-5h' | 'gemini-7d' | 'other-5h' | 'other-7d';
export type QuotaGaugeStyle = 'concentric' | 'bars' | 'classic';

export interface CategoryMeta {
  label: string;
  shortLabel: string;
  hexColor: string;
  windowSeconds: number;
}

export function getCategoryMeta(category: QuotaCategory): CategoryMeta {
  switch (category) {
    case 'gemini-5h':
      return {
        label: 'Gemini 5h',
        shortLabel: 'G-5H',
        hexColor: '#38bdf8', // Cyan / Sky
        windowSeconds: 5 * 3600, // 18,000s
      };
    case 'gemini-7d':
      return {
        label: 'Gemini 7d',
        shortLabel: 'G-7D',
        hexColor: '#3b82f6', // Blue
        windowSeconds: 7 * 86400, // 604,800s
      };
    case 'other-5h':
      return {
        label: 'Other 5h',
        shortLabel: '3P-5H',
        hexColor: '#f97316', // Amber / Orange
        windowSeconds: 5 * 3600, // 18,000s
      };
    case 'other-7d':
      return {
        label: 'Other 7d',
        shortLabel: '3P-7D',
        hexColor: '#ea580c', // Deep Coral
        windowSeconds: 7 * 86400, // 604,800s
      };
  }
}

/**
 * Calculates the fraction of time remaining in the rolling window (from 0.0 to 1.0).
 * 1.0 means the full window duration remains (just started / reset).
 * 0.0 means the window has completely elapsed and is resetting.
 */
export function calculateTimeRemainingRatio(
  resetsAt?: string,
  resetEpochSeconds?: number,
  totalWindowSeconds = 18000
): number | undefined {
  if (resetEpochSeconds !== undefined && resetEpochSeconds > 0) {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const diffSeconds = Math.max(0, resetEpochEpochDelta(resetEpochSeconds, nowSeconds));
    return Math.min(1, Math.max(0, diffSeconds / totalWindowSeconds));
  }

  if (resetsAt && typeof resetsAt === 'string') {
    const parsedSec = parseDurationToSeconds(resetsAt);
    if (parsedSec !== null) {
      return Math.min(1, Math.max(0, parsedSec / totalWindowSeconds));
    }
  }

  return undefined;
}

function resetEpochEpochDelta(resetEpochSeconds: number, nowSeconds: number): number {
  return resetEpochSeconds - nowSeconds;
}

function parseDurationToSeconds(str: string): number | null {
  const trimmed = str.trim().toLowerCase();
  let total = 0;
  let matched = false;

  const dayMatch = trimmed.match(/(\d+)\s*d/);
  if (dayMatch) {
    total += parseInt(dayMatch[1], 10) * 86400;
    matched = true;
  }

  const hourMatch = trimmed.match(/(\d+)\s*h/);
  if (hourMatch) {
    total += parseInt(hourMatch[1], 10) * 3600;
    matched = true;
  }

  const minMatch = trimmed.match(/(\d+)\s*m/);
  if (minMatch) {
    total += parseInt(minMatch[1], 10) * 60;
    matched = true;
  }

  const secMatch = trimmed.match(/(\d+)\s*s/);
  if (secMatch) {
    total += parseInt(secMatch[1], 10);
    matched = true;
  }

  return matched ? total : null;
}

/**
 * Formats a Unix epoch timestamp (in seconds) into a local time string (HH:MM or MM/DD HH:MM).
 */
export function formatLocalResetTime(epochSeconds?: number): string | undefined {
  if (!epochSeconds || epochSeconds <= 0) return undefined;
  const d = new Date(epochSeconds * 1000);
  const now = new Date();
  const isSameDay = d.toDateString() === now.toDateString();
  const hours = d.getHours().toString().padStart(2, '0');
  const minutes = d.getMinutes().toString().padStart(2, '0');
  if (isSameDay) {
    return `${hours}:${minutes}`;
  }
  const month = (d.getMonth() + 1).toString().padStart(2, '0');
  const day = d.getDate().toString().padStart(2, '0');
  return `${month}/${day} ${hours}:${minutes}`;
}

export function formatQuotaTooltip(
  label: string,
  percentage?: number,
  resetsAt?: string,
  resetEpochSeconds?: number
): string {
  if (percentage === undefined || percentage === null) {
    return `${label}: Untracked / N/A`;
  }
  const localTime = formatLocalResetTime(resetEpochSeconds);
  const localPart = localTime ? ` at ${localTime} local` : '';
  const relPart = resetsAt ? ` (${resetsAt} left)` : '';
  const timeInfo = (localPart || relPart) ? ` • Resets${localPart}${relPart}` : '';

  if (percentage <= 0) {
    return `${label}: 0% remaining (Depleted)${timeInfo}`;
  }
  const pctStr = `${percentage.toFixed(0)}% remaining`;
  return `${label}: ${pctStr}${timeInfo}`;
}
