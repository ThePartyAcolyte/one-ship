<script setup lang="ts">
import { ref, computed } from 'vue';
import AppFileTreeNode from './AppFileTreeNode.vue';
import type { FileTree } from '../../types/generated/FileTree';
import type { FileTreeNode } from '../../types/generated/FileTreeNode';

export type { FileTree, FileTreeNode };

const props = defineProps<{
  nodes: FileTreeNode[];
  modelValue: string[];
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', val: string[]): void;
}>();

const expandedDirs = ref<Record<string, boolean>>({});
const filterQuery = ref('');

function toggleDir(path: string) {
  expandedDirs.value[path] = !expandedDirs.value[path];
}

function collectAllFiles(node: FileTreeNode): string[] {
  if (!node.is_dir) {
    return [node.rel_path];
  }
  let files: string[] = [];
  if (node.children) {
    for (const child of node.children) {
      files = files.concat(collectAllFiles(child));
    }
  }
  return files;
}

function toggleCheck(node: FileTreeNode) {
  const current = [...props.modelValue];
  const targetFiles = collectAllFiles(node);
  const allAlreadyChecked = targetFiles.every(f => current.includes(f));

  if (allAlreadyChecked) {
    // Uncheck all target files
    const next = current.filter(f => !targetFiles.includes(f));
    emit('update:modelValue', next);
  } else {
    // Check all target files
    const next = Array.from(new Set([...current, ...targetFiles]));
    emit('update:modelValue', next);
  }
}

function filterTree(items: FileTreeNode[], query: string): FileTreeNode[] {
  if (!query) return items;
  const q = query.toLowerCase();
  const res: FileTreeNode[] = [];

  for (const item of items) {
    if (item.is_dir) {
      const filteredChildren = filterTree(item.children || [], query);
      if (filteredChildren.length > 0 || item.name.toLowerCase().includes(q) || item.rel_path.toLowerCase().includes(q)) {
        res.push({
          ...item,
          children: filteredChildren,
        });
        expandedDirs.value[item.rel_path] = true;
      }
    } else if (item.name.toLowerCase().includes(q) || item.rel_path.toLowerCase().includes(q)) {
      res.push(item);
    }
  }
  return res;
}

const displayNodes = computed(() => filterTree(props.nodes, filterQuery.value.trim()));

function clearSelection() {
  emit('update:modelValue', []);
}
</script>

<template>
  <div class="border border-[var(--border-color)] bg-black/40 p-2 flex flex-col gap-2 font-mono text-xs">
    <!-- Filter Search Bar -->
    <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-1.5 gap-2">
      <input 
        v-model="filterQuery" 
        type="text" 
        placeholder="Filter files by name or path..."
        class="bg-transparent border-none text-[11px] text-[var(--fg-color)] placeholder:text-[var(--dim-color)]/50 outline-none w-full font-mono"
      />
      <div class="flex items-center gap-2 shrink-0">
        <span v-if="modelValue.length" class="text-[10px] text-[var(--accent-color)] font-bold">
          {{ modelValue.length }} files
        </span>
        <button 
          v-if="modelValue.length"
          type="button" 
          @click="clearSelection" 
          class="text-[10px] text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer underline"
        >
          Clear
        </button>
      </div>
    </div>

    <!-- Tree Container (Scrollable) -->
    <div class="max-h-52 overflow-y-auto flex flex-col gap-0.5 select-none pr-1">
      <template v-if="displayNodes.length">
        <AppFileTreeNode 
          v-for="node in displayNodes"
          :key="node.rel_path"
          :node="node"
          :selected-paths="modelValue"
          :expanded-dirs="expandedDirs"
          @toggle-check="toggleCheck"
          @toggle-dir="toggleDir"
        />
      </template>
      <div v-else class="text-[11px] text-[var(--dim-color)] opacity-60 p-3 italic text-center">
        No files found matching filter
      </div>
    </div>
  </div>
</template>
