import { defineStore } from 'pinia';
import { onDaemonEvent } from '../utils/daemonEvents';
import type { SolidcraftStatus } from '../types/generated/SolidcraftStatus';
import type { SolidcraftPreviewResult } from '../types/generated/SolidcraftPreviewResult';
import type { SolidcraftMetricsResult } from '../types/generated/SolidcraftMetricsResult';
import type { PcbcraftStatus } from '../types/generated/PcbcraftStatus';
import type { PcbcraftProjectInspection } from '../types/generated/PcbcraftProjectInspection';
import type { PcbcraftSvgPreviewResult } from '../types/generated/PcbcraftSvgPreviewResult';

const MAX_PROJECT_CACHE_VARIANTS_COUNT = 16;

export type PcbPreviewMode = 'schematic' | 'board';

export interface CadProjectCache {
  status: SolidcraftStatus | null;
  selectedFile: string;
  selectedView: string;
  appliedSection: string;
  previewsByKey: Record<string, SolidcraftPreviewResult>;
  metricsByFile: Record<string, SolidcraftMetricsResult>;
  dirtyFiles: Record<string, boolean>;
  revision: number;
}

export interface PcbProjectCache {
  status: PcbcraftStatus | null;
  selectedTarget: string;
  selectedMode: PcbPreviewMode;
  selectedSheetId: string;
  previewsByKey: Record<string, PcbcraftSvgPreviewResult>;
  inspectionsByTarget: Record<string, PcbcraftProjectInspection>;
  dirtyTargets: Record<string, boolean>;
  revision: number;
}

function emptyCadCache(): CadProjectCache {
  return {
    status: null,
    selectedFile: '',
    selectedView: 'iso',
    appliedSection: '',
    previewsByKey: {},
    metricsByFile: {},
    dirtyFiles: {},
    revision: 0,
  };
}

function emptyPcbCache(): PcbProjectCache {
  return {
    status: null,
    selectedTarget: '',
    selectedMode: 'schematic',
    selectedSheetId: '',
    previewsByKey: {},
    inspectionsByTarget: {},
    dirtyTargets: {},
    revision: 0,
  };
}

function insertBounded<T>(record: Record<string, T>, key: string, value: T): void {
  if (!(key in record)) {
    const keys = Object.keys(record);
    if (keys.length >= MAX_PROJECT_CACHE_VARIANTS_COUNT) {
      delete record[keys[0]];
    }
  }
  record[key] = value;
}

export function cadPreviewKey(fileName: string, view: string, section: string): string {
  return `${fileName}|${view}|${section.trim()}`;
}

export function pcbPreviewKey(targetName: string, mode: PcbPreviewMode, sheetId: string): string {
  return `${targetName}|${mode}|${mode === 'schematic' ? sheetId : ''}`;
}

let unlistenCallbacks: (() => void)[] = [];

export const useHardwarePreviewStore = defineStore('hardwarePreview', {
  state: () => ({
    cadByProject: {} as Record<string, CadProjectCache>,
    pcbByProject: {} as Record<string, PcbProjectCache>,
  }),

  actions: {
    getCadCache(projectPath: string): CadProjectCache {
      if (!this.cadByProject[projectPath]) {
        this.cadByProject[projectPath] = emptyCadCache();
      }
      return this.cadByProject[projectPath];
    },

    getPcbCache(projectPath: string): PcbProjectCache {
      if (!this.pcbByProject[projectPath]) {
        this.pcbByProject[projectPath] = emptyPcbCache();
      }
      return this.pcbByProject[projectPath];
    },

    saveCadPreview(
      projectPath: string,
      fileName: string,
      view: string,
      section: string,
      result: SolidcraftPreviewResult
    ) {
      const cache = this.getCadCache(projectPath);
      insertBounded(cache.previewsByKey, cadPreviewKey(fileName, view, section), result);
      delete cache.dirtyFiles[fileName];
    },

    saveCadMetrics(projectPath: string, fileName: string, result: SolidcraftMetricsResult) {
      const cache = this.getCadCache(projectPath);
      insertBounded(cache.metricsByFile, fileName, result);
      delete cache.dirtyFiles[fileName];
    },

    invalidateCadFile(projectPath: string, fileName?: string) {
      const cache = this.cadByProject[projectPath];
      if (!cache) return;
      if (!fileName) {
        cache.previewsByKey = {};
        cache.metricsByFile = {};
        cache.dirtyFiles = {};
      } else {
        const prefix = `${fileName}|`;
        for (const k of Object.keys(cache.previewsByKey)) {
          if (k.startsWith(prefix)) delete cache.previewsByKey[k];
        }
        delete cache.metricsByFile[fileName];
        cache.dirtyFiles[fileName] = true;
      }
      cache.revision += 1;
    },

    savePcbPreview(
      projectPath: string,
      targetName: string,
      mode: PcbPreviewMode,
      sheetId: string,
      result: PcbcraftSvgPreviewResult
    ) {
      const cache = this.getPcbCache(projectPath);
      insertBounded(cache.previewsByKey, pcbPreviewKey(targetName, mode, sheetId), result);
      delete cache.dirtyTargets[targetName];
    },

    savePcbInspection(projectPath: string, targetName: string, result: PcbcraftProjectInspection) {
      const cache = this.getPcbCache(projectPath);
      insertBounded(cache.inspectionsByTarget, targetName, result);
      delete cache.dirtyTargets[targetName];
    },

    invalidatePcbTarget(projectPath: string, targetName?: string) {
      const cache = this.pcbByProject[projectPath];
      if (!cache) return;
      if (!targetName) {
        cache.previewsByKey = {};
        cache.inspectionsByTarget = {};
        cache.dirtyTargets = {};
      } else {
        const prefix = `${targetName}|`;
        for (const k of Object.keys(cache.previewsByKey)) {
          if (k.startsWith(prefix)) delete cache.previewsByKey[k];
        }
        delete cache.inspectionsByTarget[targetName];
        cache.dirtyTargets[targetName] = true;
      }
      cache.revision += 1;
    },

    initEventListeners() {
      if (unlistenCallbacks.length > 0) return;
      unlistenCallbacks.push(
        onDaemonEvent('solidcraft-file-changed', (payload) => {
          this.invalidateCadFile(payload.project_path, payload.file_name);
        }),
        onDaemonEvent('pcbcraft-file-changed', (payload) => {
          this.invalidatePcbTarget(payload.project_path, payload.target_name);
        })
      );
    },

    disposeEventListeners() {
      for (const unlisten of unlistenCallbacks) {
        unlisten();
      }
      unlistenCallbacks = [];
    },
  },
});
