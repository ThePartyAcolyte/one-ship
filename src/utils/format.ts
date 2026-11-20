/**
 * Shared formatting utilities for byte sizes, timestamps, durations, and token counts.
 */

export function formatBytes(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatTime(epochSeconds?: number | null): string {
  if (!epochSeconds) return '';
  return new Date(epochSeconds * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

export function formatShortTime(epochSeconds?: number | null): string {
  if (!epochSeconds) return '';
  return new Date(epochSeconds * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatDate(epochSeconds?: number | null): string {
  if (!epochSeconds) return '';
  const d = new Date(epochSeconds * 1000);
  return d.toLocaleString([], {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatEpochMs(epochMs?: number | null): string {
  if (!epochMs) return '—';
  return new Date(epochMs).toLocaleTimeString();
}

export function formatDurationMs(durationMs?: number | null): string {
  if (durationMs === undefined || durationMs === null) return '—';
  if (durationMs < 1000) return `${durationMs}ms`;
  const secs = durationMs / 1000;
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const mins = Math.floor(secs / 60);
  const remSecs = Math.round(secs % 60);
  return `${mins}m ${remSecs}s`;
}

export function formatIsoTimestamp(isoStr?: string | null): string {
  if (!isoStr) return '';
  try {
    const d = new Date(isoStr);
    if (Number.isNaN(d.getTime())) return isoStr;
    return d.toLocaleString([], {
      year: 'numeric',
      month: 'short',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  } catch {
    return isoStr;
  }
}

export function formatTokenCount(count?: number | null): string {
  if (!count || count <= 0) return '0';
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k`;
  return String(count);
}

export function formatSpeedKbps(kbs?: number | null): string {
  if (!kbs || kbs < 0.1) return '0.0 KB/s';
  if (kbs >= 1024) return `${(kbs / 1024).toFixed(1)} MB/s`;
  return `${kbs.toFixed(1)} KB/s`;
}
