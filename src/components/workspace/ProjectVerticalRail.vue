<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { 
  PanelLeft, 
  PanelLeftClose, 
  Plus, 
  Search, 
  X, 
  GitBranch, 
  Settings, 
  History, 
  Trash2, 
  FolderKanban,
  Star,
  Eye,
  EyeOff
} from '@lucide/vue';
import { useCoreStore } from '../../stores/coreStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { AppTooltip, AppInput } from '../ui';
import ProjectStatusLeds from './ProjectStatusLeds.vue';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  projects: ProjectRecord[];
}>();

const emit = defineEmits<{
  (e: 'addProject'): void;
  (e: 'openSettings', project: ProjectRecord): void;
  (e: 'openSnapshots', project: ProjectRecord): void;
  (e: 'deleteProject', project: ProjectRecord): void;
}>();

const coreStore = useCoreStore();
const layoutStore = useWorkspaceLayoutStore();

const isDrawerOpen = computed({
  get: () => layoutStore.isProjectDrawerOpen,
  set: (val: boolean) => layoutStore.setProjectDrawerOpen(val),
});
const searchQuery = ref('');
const showArchived = ref(localStorage.getItem('oneship_show_archived_projects') === 'true');

function toggleShowArchived() {
  showArchived.value = !showArchived.value;
  localStorage.setItem('oneship_show_archived_projects', String(showArchived.value));
}

function toggleDrawer() {
  isDrawerOpen.value = !isDrawerOpen.value;
  if (!isDrawerOpen.value) {
    searchQuery.value = '';
  }
}

function closeDrawer() {
  isDrawerOpen.value = false;
  searchQuery.value = '';
}

function getProjectInitials(name?: string): string {
  if (!name) return '??';
  const parts = name.split(/[-_\s]+/);
  if (parts.length >= 2) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return name.slice(0, 2).toUpperCase();
}

function selectProject(projectId: number) {
  coreStore.setActiveProject(projectId);
  closeDrawer();
}

const archivedCount = computed(() => props.projects.filter(p => p.is_archived).length);

const visibleProjects = computed(() => {
  return showArchived.value ? props.projects : props.projects.filter(p => !p.is_archived);
});

const sortedProjects = computed(() => {
  return visibleProjects.value.slice().sort((a, b) => {
    if (a.is_archived !== b.is_archived) {
      return a.is_archived ? 1 : -1;
    }
    if (a.is_favorite !== b.is_favorite) {
      return a.is_favorite ? -1 : 1;
    }
    return a.name.localeCompare(b.name);
  });
});

const filteredProjects = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return sortedProjects.value;
  return sortedProjects.value.filter(p => 
    p.name.toLowerCase().includes(q) || 
    (p.path && p.path.toLowerCase().includes(q))
  );
});

function handleKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'b') {
    e.preventDefault();
    toggleDrawer();
  } else if (e.key === 'Escape' && isDrawerOpen.value) {
    closeDrawer();
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="relative shrink-0 flex select-none z-50">
    
    <!-- ── 1. COMPACT VERTICAL PROJECT RAIL (Leftmost Strip) ───────── -->
    <div class="w-11 shrink-0 border-r border-[var(--border-color)] bg-[var(--ui-bg-card)]/50 flex flex-col items-center justify-between py-2 select-none relative z-50">
      
      <!-- Top Action Controls: Drawer Toggle & Add Project -->
      <div class="flex flex-col items-center gap-1.5 w-full px-1">
        <!-- Toggle Drawer Button -->
        <AppTooltip text="Toggle Projects Drawer [Ctrl+B]" position="right">
          <button 
            @click="toggleDrawer"
            class="w-8 h-8 flex flex-col items-center justify-center border transition-colors cursor-pointer"
            :class="isDrawerOpen 
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold' 
              : 'border-[var(--border-color)] bg-[var(--bg-color)] hover:border-[var(--accent-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)]'"
          >
            <PanelLeftClose v-if="isDrawerOpen" :size="14" />
            <PanelLeft v-else :size="14" />
          </button>
        </AppTooltip>

        <div class="w-full border-b border-[var(--border-color)]/60 my-1"></div>
      </div>

      <!-- Center: Vertical Project Buttons (Rotated 90 Degrees, Mouse-Wheel Scrollable) -->
      <div class="flex-1 flex flex-col items-center gap-3 py-2 w-full overflow-y-auto scrollbar-none">
        <div 
          v-for="project in sortedProjects" 
          :key="project.id"
          class="w-full flex justify-center shrink-0"
        >
          <AppTooltip :text="`${project.is_favorite ? '★ ' : ''}${project.is_archived ? '[ARCHIVED] ' : ''}Active Workspace: ${project.name}\n${project.path}`" position="right">
            <button
              @click="coreStore.setActiveProject(project.id)"
              class="w-8 py-3 flex flex-col items-center justify-between gap-2 border transition-all cursor-pointer group"
              :class="[
                coreStore.activeProjectId === project.id 
                  ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold shadow-[0_0_8px_color-mix(in_srgb,var(--accent-color)_15%,transparent)]' 
                  : 'border-[var(--border-color)]/70 bg-[var(--bg-color)]/70 text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--fg-color)]',
                project.is_archived ? 'opacity-50' : ''
              ]"
            >
              <!-- Project Initials Badge + Star if favorite -->
              <div class="flex flex-col items-center gap-0.5">
                <Star v-if="project.is_favorite" :size="8" class="text-[var(--status-warn-color)] fill-[var(--status-warn-color)] shrink-0" />
                <span 
                  class="text-[9px] font-mono font-bold px-0.5 tracking-tighter"
                  :class="coreStore.activeProjectId === project.id ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)] group-hover:text-[var(--accent-color)]'"
                >
                  {{ getProjectInitials(project.name) }}
                </span>
              </div>

              <!-- Status LEDs (Vertical) -->
              <ProjectStatusLeds 
                :project-id="project.id" 
                :project-path="project.path" 
                orientation="vertical" 
                :show-sockets-when-idle="true" 
              />

              <!-- Rotated Natural Project Name -->
              <span 
                class="[writing-mode:vertical-rl] rotate-180 text-[9px] uppercase tracking-[0.16em] font-mono truncate max-h-36 py-1 select-none"
                :class="coreStore.activeProjectId === project.id ? 'text-[var(--fg-color)] font-bold' : 'text-[var(--dim-color)] group-hover:text-[var(--fg-color)]'"
              >
                {{ project.name }}
              </span>

              <!-- Active Bullet Indicator -->
              <span 
                class="w-1.5 h-1.5 rounded-full"
                :class="coreStore.activeProjectId === project.id ? 'bg-[var(--accent-color)]' : 'bg-transparent'"
              />
            </button>
          </AppTooltip>
        </div>
      </div>
    </div>

    <!-- ── 2. SLIDE-OVER PROJECTS DRAWER (Overlays on Top of Workspace Content & Taskbar) ── -->
    <Teleport to="body">
      <Transition name="drawer-fade">
        <!-- Backdrop Overlay (Click Outside to Close) -->
        <div 
          v-if="isDrawerOpen" 
          class="fixed inset-0 z-40 ui-overlay-backdrop"
          @click="closeDrawer"
        />
      </Transition>
    </Teleport>

    <Transition name="drawer-slide">
      <div 
        v-if="isDrawerOpen"
        class="absolute top-0 bottom-0 left-11 z-50 w-72 sm:w-80 ui-surface-base border-t border-b border-r border-[var(--border-color)] shadow-2xl flex flex-col font-mono text-[10px] select-none h-full overflow-hidden"
        @click.stop
      >
        <!-- Drawer Header -->
        <div class="p-3 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0">
          <div class="flex items-center gap-2">
            <FolderKanban :size="14" class="text-[var(--accent-color)]" />
            <span class="font-bold text-[var(--fg-color)] uppercase tracking-wider text-[10px]">Projects Registry</span>
            <span class="px-1.5 py-0.2 ui-surface-card border border-[var(--border-color)] text-[8px] text-[var(--dim-color)]">
              {{ visibleProjects.length }}<template v-if="archivedCount > 0">/{{ projects.length }}</template>
            </span>
          </div>

          <div class="flex items-center gap-1">
            <AppTooltip
              v-if="archivedCount > 0 || showArchived"
              :text="showArchived ? 'Hide archived projects' : `Show archived projects (${archivedCount})`"
            >
              <button
                @click="toggleShowArchived"
                class="ui-icon-btn"
                :class="showArchived
                  ? '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15'
                  : ''"
              >
                <Eye v-if="showArchived" :size="12" />
                <EyeOff v-else :size="12" />
              </button>
            </AppTooltip>

            <AppTooltip text="Close drawer" position="left">
              <button 
                @click="closeDrawer"
                class="ui-icon-btn"
              >
                <X :size="13" />
              </button>
            </AppTooltip>
          </div>
        </div>

        <!-- Search Bar (Omnisearch-aligned) -->
        <div class="ui-search-box !border-x-0 !border-t-0 px-3 py-2.5 shrink-0">
          <Search :size="14" class="ui-search-box__icon" />
          <AppInput 
            v-model="searchQuery"
            type="text" 
            placeholder="Search projects..." 
            class="!bg-transparent !border-none !h-auto !px-0 !py-0 text-[10px]"
          />
          <button 
            v-if="searchQuery" 
            @click="searchQuery = ''"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer transition-colors"
          >
            <X :size="12" />
          </button>
          <kbd class="ui-kbd">ESC</kbd>
        </div>

        <!-- Projects List (Scrolleable) -->
        <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-1.5 min-h-0 ui-surface-list">
          <div 
            v-if="filteredProjects.length === 0" 
            class="p-6 text-center text-[var(--dim-color)] opacity-60 text-[9px] italic"
          >
            <template v-if="searchQuery">
              No projects found matching "{{ searchQuery }}"
            </template>
            <template v-else-if="archivedCount > 0 && !showArchived">
              All projects are archived
            </template>
            <template v-else>
              No projects registered
            </template>
          </div>

          <div
            v-for="proj in filteredProjects"
            :key="proj.id"
            @click="selectProject(proj.id)"
            class="p-2.5 border transition-all cursor-pointer flex flex-col gap-1.5 group relative"
            :class="[
              coreStore.activeProjectId === proj.id 
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 shadow-[0_0_10px_color-mix(in_srgb,var(--accent-color)_10%,transparent)]' 
                : 'border-[var(--border-color)]/70 ui-surface-card hover:border-[var(--accent-color)]/60',
              proj.is_archived ? 'opacity-60' : ''
            ]"
          >
            <!-- Project Header: Name + Active Indicator -->
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-1.5 truncate">
                <AppTooltip :text="proj.is_favorite ? 'Remove from favorites' : 'Pin to favorites'">
                  <button
                    @click.stop="coreStore.toggleProjectFavorite(proj.id, !proj.is_favorite)"
                    class="p-0.5 text-[var(--dim-color)] hover:text-[var(--status-warn-color)] transition-colors cursor-pointer shrink-0"
                    :class="proj.is_favorite ? 'text-[var(--status-warn-color)]' : 'opacity-40 hover:opacity-100'"
                  >
                    <Star :size="12" :class="{ 'fill-[var(--status-warn-color)]': proj.is_favorite }" />
                  </button>
                </AppTooltip>
                <span 
                  class="font-bold text-[11px] truncate uppercase tracking-tight"
                  :class="coreStore.activeProjectId === proj.id ? 'text-[var(--accent-color)]' : 'text-[var(--fg-color)] group-hover:text-[var(--accent-color)]'"
                >
                  {{ proj.name }}
                </span>
                <ProjectStatusLeds :project-id="proj.id" :project-path="proj.path" orientation="horizontal" />
              </div>

              <!-- Active / Archived Badge -->
              <div class="flex items-center gap-1 shrink-0">
                <span
                  v-if="proj.is_archived"
                  class="px-1.5 py-0.2 ui-surface-title border border-[var(--border-color)] text-[var(--dim-color)] text-[8px] font-bold uppercase tracking-wider"
                >
                  ARCHIVED
                </span>
                <span 
                  v-if="coreStore.activeProjectId === proj.id"
                  class="px-1.5 py-0.2 bg-[var(--accent-color)]/20 border border-[var(--accent-color)]/60 text-[var(--accent-color)] text-[8px] font-bold uppercase tracking-wider"
                >
                  ACTIVE
                </span>
              </div>
            </div>

            <!-- Path in Dim text -->
            <div class="text-[8px] text-[var(--dim-color)] truncate font-mono">
              {{ proj.path }}
            </div>

            <!-- Metadata Row: Git Branch & Quick Actions -->
            <div class="flex items-center justify-between pt-1 border-t border-[var(--border-color)]/40 text-[8px] mt-0.5">
              <!-- Git Status -->
              <div class="flex items-center gap-1 text-[var(--dim-color)]">
                <template v-if="proj.has_git && proj.git_status">
                  <GitBranch :size="10" class="text-[var(--accent-color)]" />
                  <span class="truncate max-w-[90px]">{{ proj.git_status.branch ?? 'detached' }}</span>
                  <span 
                    class="px-1 py-0.1 border text-[8px]"
                    :class="proj.git_status.is_clean ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]'"
                  >
                    {{ proj.git_status.is_clean ? 'clean' : 'modified' }}
                  </span>
                </template>
                <AppTooltip v-else-if="proj.has_git" :text="proj.git_error ?? 'Git status unavailable'" position="right">
                  <span class="font-bold text-[var(--status-error-color)] cursor-default">[! GIT]</span>
                </AppTooltip>
                <template v-else>
                  <span class="opacity-50">no vcs</span>
                </template>
              </div>

              <!-- Project Card Actions -->
              <div class="flex items-center gap-1" @click.stop>
                <AppTooltip text="Project Settings">
                  <button 
                    @click="emit('openSettings', proj)"
                    class="p-1 hover:text-[var(--accent-color)] text-[var(--dim-color)] transition-colors cursor-pointer"
                  >
                    <Settings :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip v-if="proj.has_git" text="Snapshots & Diff">
                  <button 
                    @click="emit('openSnapshots', proj)"
                    class="p-1 hover:text-[var(--accent-color)] text-[var(--dim-color)] transition-colors cursor-pointer"
                  >
                    <History :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip :text="proj.is_archived ? 'Restore project' : 'Archive (hide) project'">
                  <button
                    @click="coreStore.toggleProjectArchived(proj.id, !proj.is_archived)"
                    class="p-1 transition-colors cursor-pointer"
                    :class="proj.is_archived
                      ? 'text-[var(--accent-color)] hover:opacity-80'
                      : 'text-[var(--dim-color)] hover:text-[var(--accent-color)]'"
                  >
                    <Eye v-if="proj.is_archived" :size="11" />
                    <EyeOff v-else :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip text="Unregister Project">
                  <button 
                    @click="emit('deleteProject', proj)"
                    class="p-1 hover:text-[var(--status-error-color)] text-[var(--dim-color)] transition-colors cursor-pointer"
                  >
                    <Trash2 :size="11" />
                  </button>
                </AppTooltip>
              </div>
            </div>
          </div>
        </div>

        <!-- Drawer Footer: Add Project Button -->
        <div class="p-2 border-t border-[var(--border-color)] ui-surface-title shrink-0">
          <button 
            @click="() => { emit('addProject'); closeDrawer(); }"
            class="w-full h-7 px-3 border border-dashed border-[var(--border-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] text-[var(--dim-color)] font-bold uppercase text-[9px] flex items-center justify-center gap-1.5 transition-colors cursor-pointer"
          >
            <Plus :size="12" />
            <span>Track New Project</span>
          </button>
        </div>
      </div>
    </Transition>

  </div>
</template>

<style scoped>
/* Slide transition for the drawer */
.drawer-slide-enter-active,
.drawer-slide-leave-active {
  transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.2s ease;
}
.drawer-slide-enter-from,
.drawer-slide-leave-to {
  transform: translateX(-16px);
  opacity: 0;
}

/* Fade transition for backdrop */
.drawer-fade-enter-active,
.drawer-fade-leave-active {
  transition: opacity 0.2s ease;
}
.drawer-fade-enter-from,
.drawer-fade-leave-to {
  opacity: 0;
}
</style>
