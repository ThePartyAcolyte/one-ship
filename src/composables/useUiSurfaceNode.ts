import { computed } from 'vue';
import { useWorkspaceLayoutStore } from '../stores/workspaceLayoutStore';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';

/**
 * Binds a workspace subsystem component to its declarative `UiSurfaceNode` subtree (Spec 35).
 * Reads the active child segment from `workspaceLayoutStore.activeNodePath` and dispatches
 * `NavigateToNode` transitions to `oneship-daemon` on selection.
 */
export function useUiSurfaceNode<T extends string = string>(
  parentPath: string,
  defaultChildId: T
) {
  const layoutStore = useWorkspaceLayoutStore();
  const prefix = `${parentPath}/`;

  const activeChildId = computed<T>(() => {
    const fullPath = layoutStore.activeNodePath || '';
    if (fullPath.startsWith(prefix)) {
      const remainder = fullPath.slice(prefix.length);
      const nextSegment = remainder.split('/')[0];
      if (nextSegment) {
        return nextSegment as T;
      }
    }
    return defaultChildId;
  });

  function selectChild(childId: T): void {
    const targetPath = `${parentPath}/${childId}`;
    layoutStore.navigateToNode(targetPath).catch(reportAsyncFailure);
  }

  return {
    activeChildId,
    selectChild,
  };
}
