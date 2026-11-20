import type { WorkspaceTab } from '../stores/workspaceLayoutStore';

/**
 * A navigation attempt that a mounted panel may need to confirm before it is applied.
 * Exactly one of `projectId` / `workspaceTab` is set per attempt; a confirmed flow may
 * chain a project switch and a tab switch explicitly.
 */
export interface NavigationTarget {
  projectId?: number | null;
  workspaceTab?: WorkspaceTab;
}

/** Returns `true` to allow the navigation, `false` to cancel it. */
type NavigationGuard = (target: NavigationTarget) => boolean;

let activeGuard: NavigationGuard | null = null;

/**
 * Registers the single active UI navigation guard (the mounted settings panel while it has
 * unsaved configuration changes). Returns an idempotent unregister function.
 */
export function registerNavigationGuard(guard: NavigationGuard): () => void {
  activeGuard = guard;
  return () => {
    if (activeGuard === guard) {
      activeGuard = null;
    }
  };
}

/** Runs the active guard. With no registered guard every navigation is allowed. */
export function runNavigationGuard(target: NavigationTarget): boolean {
  return activeGuard ? activeGuard(target) : true;
}
