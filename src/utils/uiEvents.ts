/**
 * Strongly-typed UI event bus replacing untyped window.dispatchEvent CustomEvents (TECHNICAL_DEBT #8).
 */

import type { ProjectRecord } from '../types';
import type { AgentPersonaState } from '../components/modals/AddPersonaModal.vue';

export interface UiEventMap {
  'oneship-add-persona': ProjectRecord | undefined;
  'oneship-edit-persona': { project: ProjectRecord; agent: AgentPersonaState };
  'oneship-open-history': ProjectRecord | undefined;
  'oneship-open-task-report': { project: ProjectRecord; execId: number };
  'oneship-refresh-project': void;
  'switch-to-workspace-terminal': { terminalKey?: string; projectPath?: string } | void;
  'open-workspace-file': { filePath: string; line?: number };
  'open-workspace-devlog': { date?: string } | void;
  'open-status-log': void;
  'oneship-appearance-updated': void;
}

export type UiEventKey = keyof UiEventMap;

/**
 * Dispatches a strongly-typed UI event across components.
 */
export function emitUiEvent<K extends UiEventKey>(
  event: K,
  ...args: UiEventMap[K] extends void ? [] : [payload: UiEventMap[K]]
): void {
  const detail = args[0];
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(event, { detail }));
  }
}

/**
 * Registers a strongly-typed listener for a UI event.
 * Returns an unlisten function for deterministic lifecycle teardown.
 */
export function onUiEvent<K extends UiEventKey>(
  event: K,
  handler: (payload: UiEventMap[K]) => void
): () => void {
  if (typeof window === 'undefined') {
    return () => {};
  }
  const listener = (e: Event) => {
    const customEvent = e as CustomEvent<UiEventMap[K]>;
    handler(customEvent.detail);
  };
  window.addEventListener(event, listener);
  return () => {
    window.removeEventListener(event, listener);
  };
}
