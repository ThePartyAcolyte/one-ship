<script setup lang="ts">
import { computed } from 'vue';
import { 
  Folder, 
  FolderOpen, 
  FileText, 
  ChevronRight, 
  ChevronDown, 
  CheckSquare, 
  SquareMinus, 
  Square 
} from '@lucide/vue';
import type { FileTreeNode } from './AppFileTree.vue';

const props = defineProps<{
  node: FileTreeNode;
  selectedPaths: string[];
  expandedDirs: Record<string, boolean>;
  depth?: number;
}>();

const emit = defineEmits<{
  (e: 'toggleCheck', node: FileTreeNode): void;
  (e: 'toggleDir', path: string): void;
}>();

const currentDepth = computed(() => props.depth || 0);

function collectFiles(node: FileTreeNode): string[] {
  if (!node.is_dir) {
    return [node.rel_path];
  }
  let files: string[] = [];
  if (node.children) {
    for (const child of node.children) {
      files = files.concat(collectFiles(child));
    }
  }
  return files;
}

const descendantFiles = computed(() => collectFiles(props.node));

const checkState = computed<'checked' | 'indeterminate' | 'unchecked'>(() => {
  const files = descendantFiles.value;
  if (files.length === 0) return 'unchecked';
  
  const all = files.every(f => props.selectedPaths.includes(f));
  if (all) return 'checked';
  
  const some = files.some(f => props.selectedPaths.includes(f));
  if (some) return 'indeterminate';
  
  return 'unchecked';
});

const isExpanded = computed(() => !!props.expandedDirs[props.node.rel_path]);
</script>

<template>
  <div class="flex flex-col select-none">
    <!-- Node Row -->
    <div 
      class="flex items-center gap-1.5 py-0.5 px-1 hover:bg-[var(--accent-color)]/10 cursor-pointer rounded-none group transition-colors"
      :style="{ paddingLeft: `${currentDepth * 14 + 4}px` }"
      @click="node.is_dir ? emit('toggleDir', node.rel_path) : emit('toggleCheck', node)"
    >
      <!-- Tri-State Checkbox Button -->
      <button 
        type="button" 
        class="text-[var(--dim-color)] group-hover:text-[var(--accent-color)] cursor-pointer shrink-0"
        @click.stop="emit('toggleCheck', node)"
      >
        <CheckSquare v-if="checkState === 'checked'" :size="13" class="text-[var(--accent-color)]" />
        <SquareMinus v-else-if="checkState === 'indeterminate'" :size="13" class="text-[var(--accent-color)]" />
        <Square v-else :size="13" />
      </button>

      <!-- Expand/Collapse Chevron (Dirs only) -->
      <span v-if="node.is_dir" class="text-[var(--dim-color)] w-3 flex items-center justify-center shrink-0">
        <ChevronDown v-if="isExpanded" :size="12" />
        <ChevronRight v-else :size="12" />
      </span>
      <span v-else class="w-3 shrink-0" />

      <!-- Folder / File Icon -->
      <FolderOpen v-if="node.is_dir && isExpanded" :size="13" class="text-[var(--status-warn-color)] shrink-0" />
      <Folder v-else-if="node.is_dir" :size="13" class="text-[var(--status-warn-color)]/80 shrink-0" />
      <FileText v-else :size="13" class="text-[var(--dim-color)] shrink-0" />

      <!-- Name -->
      <span 
        class="truncate text-[11px] font-mono"
        :class="checkState === 'checked' 
          ? 'text-[var(--accent-color)] font-bold' 
          : (checkState === 'indeterminate' 
              ? 'text-[var(--fg-color)] font-bold' 
              : (node.is_dir ? 'text-[var(--fg-color)] font-semibold' : 'text-[var(--dim-color)]'))"
      >
        {{ node.name }}
      </span>
    </div>

    <!-- Recursive Children (Infinite Depth) -->
    <div v-if="node.is_dir && isExpanded && node.children?.length" class="flex flex-col">
      <AppFileTreeNode 
        v-for="child in node.children" 
        :key="child.rel_path"
        :node="child"
        :selected-paths="selectedPaths"
        :expanded-dirs="expandedDirs"
        :depth="currentDepth + 1"
        @toggle-check="(n) => emit('toggleCheck', n)"
        @toggle-dir="(p) => emit('toggleDir', p)"
      />
    </div>
  </div>
</template>
