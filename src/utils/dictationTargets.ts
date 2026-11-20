// Dictation targets: the focused text field or a reactive value owned by a panel. A target keeps
// its base value and selection so interim Live transcripts replace only their provisional span.

export type DictationTargetKind = 'field' | 'chat' | 'comment' | 'settings';

export interface DictationTargetMeta {
  kind: DictationTargetKind;
  agentId?: string | null;
  projectPath?: string | null;
}

export interface DictationTarget {
  key: string;
  meta: DictationTargetMeta;
  focus(): void;
  apply(text: string): boolean;
  restore(): void;
  isPristine(): boolean;
}

const TEXT_INPUT_TYPES = new Set([
  'text',
  'search',
  'url',
  'email',
  'tel',
]);

export function isDictationElement(element: Element | null): element is HTMLInputElement | HTMLTextAreaElement {
  if (!element) return false;
  if (element instanceof HTMLTextAreaElement) {
    return !element.disabled && !element.readOnly;
  }
  if (element instanceof HTMLInputElement) {
    if (element.disabled || element.readOnly) return false;
    return TEXT_INPUT_TYPES.has(element.type);
  }
  return false;
}

function dispatchInput(element: HTMLInputElement | HTMLTextAreaElement): void {
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

export function createDomTarget(
  key: string,
  element: HTMLInputElement | HTMLTextAreaElement,
  meta: DictationTargetMeta,
): DictationTarget {
  const baseValue = element.value;
  const start = element.selectionStart ?? baseValue.length;
  const end = element.selectionEnd ?? start;
  let lastApplied = baseValue;

  const write = (value: string, caret: number) => {
    element.value = value;
    dispatchInput(element);
    const position = Math.min(caret, value.length);
    element.setSelectionRange(position, position);
    element.scrollTop = element.scrollHeight;
    lastApplied = value;
  };

  return {
    key,
    meta,
    focus: () => element.focus(),
    apply: (text: string) => {
      if (!element.isConnected || element.value !== lastApplied) {
        return false;
      }
      const next = `${baseValue.slice(0, start)}${text}${baseValue.slice(end)}`;
      write(next, start + text.length);
      return true;
    },
    restore: () => {
      if (element.isConnected) {
        write(baseValue, start);
      }
    },
    isPristine: () => element.isConnected && element.value === lastApplied,
  };
}

export function createValueTarget(
  key: string,
  meta: DictationTargetMeta,
  read: () => string,
  write: (value: string) => void,
): DictationTarget {
  const baseValue = read();
  let lastApplied = baseValue;

  return {
    key,
    meta,
    focus: () => {},
    apply: (text: string) => {
      if (read() !== lastApplied) {
        return false;
      }
      const next = baseValue.trimEnd().length > 0
        ? `${baseValue.trimEnd()} ${text}`
        : text;
      write(next);
      lastApplied = next;
      return true;
    },
    restore: () => {
      write(baseValue);
      lastApplied = baseValue;
    },
    isPristine: () => read() === lastApplied,
  };
}
