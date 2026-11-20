/**
 * Robust, lightweight ANSI escape sequence parser and HTML converter.
 * Maps standard 16 ANSI colors, bright colors, 256 colors, and styles
 * (bold, dim, underline, italic) to brutalist terminal styles.
 */

const ANSI_COLOR_MAP: Record<number, string> = {
  30: 'text-[var(--bg-color)]',
  31: 'text-[var(--status-error-color)] font-medium',
  32: 'text-[var(--status-ok-color)] font-medium',
  33: 'text-[var(--status-warn-color)] font-medium',
  34: 'text-[var(--status-info-color)] font-medium',
  35: 'text-[var(--accent-color)] font-medium',
  36: 'text-[var(--status-info-color)] font-medium',
  37: 'text-[var(--fg-color)]',
  90: 'text-[var(--dim-color)]',
  91: 'text-[var(--status-error-color)]',
  92: 'text-[var(--status-ok-color)]',
  93: 'text-[var(--status-warn-color)]',
  94: 'text-[var(--status-info-color)]',
  95: 'text-[var(--accent-color)]',
  96: 'text-[var(--status-info-color)]',
  97: 'text-white font-semibold',
};

const ANSI_BG_MAP: Record<number, string> = {
  40: 'bg-[var(--bg-color)]',
  41: 'bg-[var(--status-error-bg)] text-[var(--status-error-color)]',
  42: 'bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]',
  43: 'bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]',
  44: 'bg-[var(--status-info-bg)] text-[var(--status-info-color)]',
  45: 'bg-[var(--accent-color)]/20 text-[var(--accent-color)]',
  46: 'bg-[var(--status-info-bg)] text-[var(--status-info-color)]',
  47: 'bg-[var(--fg-color)] text-[var(--bg-color)]',
  100: 'bg-[var(--border-color)]',
  101: 'bg-[var(--status-error-color)] text-white',
  102: 'bg-[var(--status-ok-color)] text-white',
  103: 'bg-[var(--status-warn-color)] text-black',
  104: 'bg-[var(--status-info-color)] text-black',
  105: 'bg-[var(--accent-color)] text-black',
  106: 'bg-[var(--status-info-color)] text-black',
  107: 'bg-white text-[var(--bg-color)]',
};

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

/**
 * Parses raw terminal text with ANSI escape sequences into sanitized HTML.
 */
export function parseAnsiToHtml(rawText: string): string {
  if (!rawText) return '';

  // 1. Strip non-SGR control escape sequences (cursor movements, screen clear, etc.)
  const sanitized = rawText.replace(/\x1b\[[0-9;]*[A-IK-Z]/g, '');

  // 2. Tokenize by SGR escape sequences: \x1b[ ... m
  const sgrRegex = /\x1b\[([0-9;]*)m/g;
  const parts: string[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  // Active styles state
  let currentClasses: Set<string> = new Set();
  let currentCustomFg: string | null = null;
  let currentCustomBg: string | null = null;

  function buildSpanStart(): string {
    const classList = Array.from(currentClasses);
    const styles: string[] = [];
    if (currentCustomFg) styles.push(`color:${currentCustomFg}`);
    if (currentCustomBg) styles.push(`background-color:${currentCustomBg}`);

    if (classList.length === 0 && styles.length === 0) {
      return '';
    }

    const classAttr = classList.length > 0 ? ` class="${classList.join(' ')}"` : '';
    const styleAttr = styles.length > 0 ? ` style="${styles.join(';')}"` : '';
    return `<span${classAttr}${styleAttr}>`;
  }

  while ((match = sgrRegex.exec(sanitized)) !== null) {
    const textChunk = sanitized.slice(lastIndex, match.index);
    if (textChunk) {
      const spanStart = buildSpanStart();
      if (spanStart) {
        parts.push(`${spanStart}${escapeHtml(textChunk)}</span>`);
      } else {
        parts.push(escapeHtml(textChunk));
      }
    }

    // Process SGR codes
    const codeStr = match[1] || '0';
    const codes = codeStr.split(';').map(c => parseInt(c, 10) || 0);

    for (let i = 0; i < codes.length; i++) {
      const code = codes[i];

      if (code === 0) {
        // Reset all styles
        currentClasses.clear();
        currentCustomFg = null;
        currentCustomBg = null;
      } else if (code === 1) {
        currentClasses.add('font-bold text-[var(--fg-color)]');
      } else if (code === 2) {
        currentClasses.add('opacity-60');
      } else if (code === 3) {
        currentClasses.add('italic');
      } else if (code === 4) {
        currentClasses.add('underline');
      } else if (code === 22) {
        currentClasses.delete('font-bold');
        currentClasses.delete('opacity-60');
      } else if (code === 23) {
        currentClasses.delete('italic');
      } else if (code === 24) {
        currentClasses.delete('underline');
      } else if (code >= 30 && code <= 37) {
        // Remove prior fg classes
        for (const c of Object.values(ANSI_COLOR_MAP)) currentClasses.delete(c);
        currentCustomFg = null;
        currentClasses.add(ANSI_COLOR_MAP[code]);
      } else if (code === 39) {
        // Default FG
        for (const c of Object.values(ANSI_COLOR_MAP)) currentClasses.delete(c);
        currentCustomFg = null;
      } else if (code >= 40 && code <= 47) {
        // Standard BG
        for (const c of Object.values(ANSI_BG_MAP)) currentClasses.delete(c);
        currentCustomBg = null;
        currentClasses.add(ANSI_BG_MAP[code]);
      } else if (code === 49) {
        // Default BG
        for (const c of Object.values(ANSI_BG_MAP)) currentClasses.delete(c);
        currentCustomBg = null;
      } else if (code >= 90 && code <= 97) {
        // Bright FG
        for (const c of Object.values(ANSI_COLOR_MAP)) currentClasses.delete(c);
        currentCustomFg = null;
        currentClasses.add(ANSI_COLOR_MAP[code]);
      } else if (code >= 100 && code <= 107) {
        // Bright BG
        for (const c of Object.values(ANSI_BG_MAP)) currentClasses.delete(c);
        currentCustomBg = null;
        currentClasses.add(ANSI_BG_MAP[code]);
      } else if (code === 38 && codes[i + 1] === 5 && codes[i + 2] !== undefined) {
        // 256 Color FG: 38;5;N
        const color256 = codes[i + 2];
        i += 2;
        currentCustomFg = get256ColorHex(color256);
      } else if (code === 48 && codes[i + 1] === 5 && codes[i + 2] !== undefined) {
        // 256 Color BG: 48;5;N
        const color256 = codes[i + 2];
        i += 2;
        currentCustomBg = get256ColorHex(color256);
      }
    }

    lastIndex = sgrRegex.lastIndex;
  }

  // Trailing text chunk
  const remainingText = sanitized.slice(lastIndex);
  if (remainingText) {
    const spanStart = buildSpanStart();
    if (spanStart) {
      parts.push(`${spanStart}${escapeHtml(remainingText)}</span>`);
    } else {
      parts.push(escapeHtml(remainingText));
    }
  }

  return parts.join('');
}

/**
 * Standard 256-color palette approximation.
 */
function get256ColorHex(code: number): string {
  if (code < 16) {
    const basic16 = [
      '#000000', '#cd0000', '#00cd00', '#cdcd00', '#0000ee', '#cd00cd', '#00cdcd', '#e5e5e5',
      '#7f7f7f', '#ff0000', '#00ff00', '#ffff00', '#5c5cff', '#ff00ff', '#00ffff', '#ffffff'
    ];
    return basic16[code] || '#cccccc';
  }
  if (code >= 16 && code <= 231) {
    const c = code - 16;
    const b = c % 6;
    const g = Math.floor(c / 6) % 6;
    const r = Math.floor(c / 36);
    const cv = [0, 95, 135, 175, 215, 255];
    return `rgb(${cv[r]},${cv[g]},${cv[b]})`;
  }
  if (code >= 232 && code <= 255) {
    const gray = 8 + (code - 232) * 10;
    return `rgb(${gray},${gray},${gray})`;
  }
  return '#cccccc';
}
