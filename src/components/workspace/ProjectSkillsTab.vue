<script setup lang="ts">
import { ref, watch, computed, onMounted } from 'vue';
import { daemonCall } from '../../utils/daemonClient';
import {
  BrainCircuit,
  Search,
  Download,
  Trash2,
  Eye,
  ExternalLink,
  Check,
  Loader2,
  AlertCircle,
  ShieldCheck,
  Sparkles,
  RotateCw,
  X,
  Link,
  LayoutGrid,
  List,
  Star,
} from '@lucide/vue';
import { AppTooltip, AppToggle, AppInput, AppEmptyState, SubtabBar, type SubtabItem } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { useUiSurfaceNode } from '../../composables/useUiSurfaceNode';
import type { ProjectRecord, ProjectSkillRecord, MarketplaceSkillItem, AgentState } from '../../types';

const VIEW_MODE_STORAGE_KEY = 'oneship.skills.view_mode';

const props = defineProps<{
  project: ProjectRecord | null;
}>();

const emit = defineEmits<{
  (e: 'skillsChanged'): void;
}>();

const coreStore = useCoreStore();

const skillsSurface = useUiSurfaceNode<'installed' | 'explore'>('skills', 'installed');
const activeSubTab = computed({
  get: () => skillsSurface.activeChildId.value,
  set: (val) => skillsSurface.selectChild(val),
});

function loadInitialViewMode(): 'grid' | 'list' {
  try {
    const saved = localStorage.getItem(VIEW_MODE_STORAGE_KEY);
    return saved === 'list' ? 'list' : 'grid';
  } catch (err: unknown) {
    console.warn('[ProjectSkillsTab] Unable to read saved view mode from localStorage:', err);
    return 'grid';
  }
}

const viewMode = ref<'grid' | 'list'>(loadInitialViewMode());
function setViewMode(mode: 'grid' | 'list') {
  viewMode.value = mode;
  try {
    localStorage.setItem(VIEW_MODE_STORAGE_KEY, mode);
  } catch (err: unknown) {
    console.warn('[ProjectSkillsTab] Unable to persist view mode to localStorage:', err);
  }
}

const installedSkills = ref<ProjectSkillRecord[]>([]);
const isLoadingInstalled = ref(false);
const filterQuery = ref('');

const nameInput = ref('');
const isInstallingUrl = ref(false);
const autoDerivedName = ref('');

const searchQuery = ref('');
const searchResults = ref<MarketplaceSkillItem[]>([]);
const isSearching = ref(false);
const hasSearched = ref(false);
const installingSkillId = ref<string | null>(null);

const isExploreUrlInput = computed(() => /^https?:\/\//i.test(searchQuery.value.trim()));

const activeSearchModel = computed({
  get: () => (activeSubTab.value === 'installed' ? filterQuery.value : searchQuery.value),
  set: (val: string) => {
    if (activeSubTab.value === 'installed') {
      filterQuery.value = val;
    } else {
      searchQuery.value = val;
    }
  },
});

function clearActiveSearch() {
  if (activeSubTab.value === 'installed') {
    filterQuery.value = '';
  } else {
    searchQuery.value = '';
    nameInput.value = '';
    autoDerivedName.value = '';
    searchResults.value = [];
    hasSearched.value = false;
  }
}

function deriveSkillNameFromUrl(url: string): string {
  if (!url) return '';
  const clean = url.trim().replace(/^https?:\/\//, '');
  const segments = clean.split('/');
  const host = segments[0] || '';
  const hostParts = host.split('.');
  if (hostParts.length >= 2) {
    if (hostParts[0] === 'docs' && hostParts.length >= 3) {
      return hostParts[1].toLowerCase();
    }
    return hostParts[0].toLowerCase();
  }
  return host.toLowerCase();
}

watch(searchQuery, (newVal) => {
  const trimmed = newVal.trim();
  if (!/^https?:\/\//i.test(trimmed)) {
    if (nameInput.value === autoDerivedName.value) {
      nameInput.value = '';
    }
    autoDerivedName.value = '';
    return;
  }
  const derived = deriveSkillNameFromUrl(trimmed);
  if (!nameInput.value || nameInput.value === autoDerivedName.value) {
    nameInput.value = derived;
    autoDerivedName.value = derived;
  }
});

async function handleInstallFromUrl() {
  const url = searchQuery.value.trim();
  if (!url || !props.project?.path) return;
  isInstallingUrl.value = true;
  errorMessage.value = null;
  successMessage.value = null;
  const targetName = nameInput.value.trim() || deriveSkillNameFromUrl(url) || 'custom-skill';
  try {
    const res: string = await daemonCall('install_project_skill', {
      projectPath: props.project.path,
      skillName: targetName,
      sourceUrl: url,
    });
    successMessage.value = res;
    searchQuery.value = '';
    nameInput.value = '';
    autoDerivedName.value = '';
    await fetchInstalledSkills();
    emit('skillsChanged');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isInstallingUrl.value = false;
  }
}

async function handleExploreSubmit() {
  if (isExploreUrlInput.value) {
    await handleInstallFromUrl();
  } else {
    await handleSearch();
  }
}

const projectAgents = computed<AgentState[]>(() => {
  if (!props.project?.id) return [];
  return coreStore.agentStates[props.project.id]?.agents || [];
});

function getAssignedAgentsForSkill(skillName: string): string[] {
  return projectAgents.value
    .filter((a) => a.skills && a.skills.includes(skillName))
    .map((a) => a.id);
}

const previewSkillName = ref<string | null>(null);
const previewContent = ref<string | null>(null);
const isLoadingPreview = ref(false);

const errorMessage = ref<string | null>(null);
const successMessage = ref<string | null>(null);

const quickSearchTags = ['stripe', 'pinia', 'meilisearch', 'webrtc', 'docker', 'prisma', 'threejs', 'vitest'];

const activeSkillsCount = computed(() => installedSkills.value.filter((s) => s.is_enabled).length);

const totalTokensEstimate = computed(() =>
  installedSkills.value
    .filter((s) => s.is_enabled)
    .reduce((acc, curr) => acc + curr.token_estimate_count, 0)
);

const filteredInstalledSkills = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q) return installedSkills.value;
  return installedSkills.value.filter(
    (s) =>
      s.name.toLowerCase().includes(q) ||
      (s.description && s.description.toLowerCase().includes(q))
  );
});

watch(
  () => props.project?.path,
  async (newPath) => {
    if (newPath) {
      errorMessage.value = null;
      successMessage.value = null;
      previewSkillName.value = null;
      previewContent.value = null;
      await fetchInstalledSkills();
    }
  },
  { immediate: true }
);

onMounted(async () => {
  if (props.project?.path) {
    await fetchInstalledSkills();
  }
});

async function fetchInstalledSkills() {
  if (!props.project?.path) return;
  isLoadingInstalled.value = true;
  errorMessage.value = null;
  try {
    const list: ProjectSkillRecord[] = await daemonCall('get_project_skills', {
      projectPath: props.project.path,
    });
    installedSkills.value = list;
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isLoadingInstalled.value = false;
  }
}

async function handleToggleSkill(skill: ProjectSkillRecord) {
  if (!props.project?.path || skill.is_platform_base) return;
  errorMessage.value = null;
  const targetState = !skill.is_enabled;
  try {
    const res: boolean = await daemonCall('toggle_project_skill', {
      projectPath: props.project.path,
      skillName: skill.name,
      enabled: targetState,
    });
    skill.is_enabled = res;
    emit('skillsChanged');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  }
}

async function handleRemoveSkill(skill: ProjectSkillRecord) {
  if (!props.project?.path || skill.is_platform_base) return;
  if (!confirm(`Are you sure you want to remove skill "${skill.name}" from this project?`)) {
    return;
  }
  errorMessage.value = null;
  try {
    await daemonCall('remove_project_skill', {
      projectPath: props.project.path,
      skillName: skill.name,
    });
    installedSkills.value = installedSkills.value.filter((s) => s.name !== skill.name);
    if (previewSkillName.value === skill.name) {
      previewSkillName.value = null;
      previewContent.value = null;
    }
    successMessage.value = `Skill "${skill.name}" removed successfully.`;
    emit('skillsChanged');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  }
}

async function handleInspectSkill(skill: ProjectSkillRecord) {
  if (!props.project?.path) return;
  previewSkillName.value = skill.name;
  isLoadingPreview.value = true;
  previewContent.value = null;
  try {
    const content: string = await daemonCall('get_project_skill_content', {
      projectPath: props.project.path,
      skillName: skill.name,
    });
    previewContent.value = content;
  } catch (err: unknown) {
    previewContent.value = `Failed to load skill content: ${err}`;
  } finally {
    isLoadingPreview.value = false;
  }
}

async function handleSearch(tag?: string) {
  if (tag) {
    searchQuery.value = tag;
  }
  const q = searchQuery.value.trim();
  if (!q) return;

  isSearching.value = true;
  errorMessage.value = null;
  hasSearched.value = true;
  try {
    const items: MarketplaceSkillItem[] = await daemonCall('search_marketplace_skills', {
      query: q,
      limitCount: 15,
    });
    searchResults.value = items;
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isSearching.value = false;
  }
}

async function handleInstallSkill(item: MarketplaceSkillItem) {
  if (!props.project?.path) return;
  installingSkillId.value = item.id;
  errorMessage.value = null;
  successMessage.value = null;
  try {
    const res: string = await daemonCall('install_project_skill', {
      projectPath: props.project.path,
      skillName: item.name,
      sourceUrl: item.github_url,
    });
    successMessage.value = res;
    await fetchInstalledSkills();
    emit('skillsChanged');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    installingSkillId.value = null;
  }
}

function isSkillInstalled(name: string): boolean {
  return installedSkills.value.some((s) => s.name.toLowerCase() === name.toLowerCase());
}
</script>

<template>
  <div class="flex-1 min-h-0 h-full flex flex-col font-mono select-text bg-[var(--bg-color)] overflow-hidden">
    <!-- Single h-9 Panel Bar with Tabs, Inline Search & View Controls (Spec 13 §4.3) -->
    <SubtabBar
      :model-value="activeSubTab"
      @update:model-value="activeSubTab = $event as 'installed' | 'explore'"
      :items="[
        { id: 'installed', label: 'Installed', icon: BrainCircuit, badge: installedSkills.length },
        { id: 'explore', label: 'Explore', icon: Sparkles },
      ] satisfies SubtabItem[]"
    >
      <template #center>
        <div class="flex items-center gap-1.5 w-full min-w-0">
          <Link
            v-if="activeSubTab === 'explore' && isExploreUrlInput"
            :size="10"
            class="text-[var(--accent-color)] shrink-0"
          />
          <Search v-else :size="10" class="text-[var(--accent-color)] shrink-0" />
          <AppInput
            v-model="activeSearchModel"
            type="text"
            :placeholder="activeSubTab === 'installed' ? 'Filter installed skills...' : 'Search SkillsMP or paste https:// URL (Enter)...'"
            class="!bg-transparent !border-none !h-6 !px-0 !py-0 text-[10px] flex-1 min-w-0"
            @keydown.enter="activeSubTab === 'explore' && handleExploreSubmit()"
          />
          <AppInput
            v-if="activeSubTab === 'explore' && isExploreUrlInput"
            v-model="nameInput"
            type="text"
            placeholder="Skill name..."
            class="w-28 sm:w-36 shrink-0 !bg-transparent !border-0 !border-l !border-[var(--border-color)]/60 !h-6 !pl-2 !pr-0 !py-0 text-[10px]"
            @keydown.enter="handleExploreSubmit()"
          />
          <button
            v-if="activeSearchModel"
            type="button"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
            @click="clearActiveSearch()"
          >
            <X :size="10" />
          </button>
          <AppTooltip
            v-if="activeSubTab === 'explore'"
            :text="isExploreUrlInput ? 'Install skill from URL' : 'Search SkillsMP registry'"
          >
            <button
              type="button"
              @click="handleExploreSubmit()"
              :disabled="(isExploreUrlInput ? isInstallingUrl : isSearching) || !searchQuery.trim()"
              class="h-6 px-2 text-[9px] font-bold uppercase border flex items-center gap-1 cursor-pointer disabled:opacity-40 shrink-0 transition-colors"
              :class="
                isExploreUrlInput
                  ? 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'
                  : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)]'
              "
            >
              <Loader2 v-if="isExploreUrlInput ? isInstallingUrl : isSearching" :size="10" class="animate-spin" />
              <Download v-else-if="isExploreUrlInput" :size="10" />
              <Search v-else :size="10" />
              <span>{{ isExploreUrlInput ? 'Install' : 'Search' }}</span>
            </button>
          </AppTooltip>
        </div>
      </template>

      <template #right>
        <div class="flex items-center gap-2 text-[9px] text-[var(--dim-color)] whitespace-nowrap shrink-0">
          <!-- Dual View Mode (Grid default / List) -->
          <div class="flex items-center gap-1">
            <AppTooltip text="Grid view">
              <button
                type="button"
                @click="setViewMode('grid')"
                class="ui-icon-btn"
                :class="viewMode === 'grid' ? '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15' : ''"
              >
                <LayoutGrid :size="12" />
              </button>
            </AppTooltip>
            <AppTooltip text="List view">
              <button
                type="button"
                @click="setViewMode('list')"
                class="ui-icon-btn"
                :class="viewMode === 'list' ? '!border-[var(--accent-color)] !text-[var(--accent-color)] !bg-[var(--accent-color)]/15' : ''"
              >
                <List :size="12" />
              </button>
            </AppTooltip>
          </div>

          <AppTooltip text="Refresh skills">
            <button
              type="button"
              @click="fetchInstalledSkills()"
              :disabled="isLoadingInstalled"
              class="ui-icon-btn disabled:opacity-40"
            >
              <RotateCw :size="12" :class="{ 'animate-spin': isLoadingInstalled }" />
            </button>
          </AppTooltip>

          <span class="hidden md:inline tabular-nums">
            Active <strong class="text-[var(--status-ok-color)]">{{ activeSkillsCount }}</strong>
            &middot;
            <strong class="text-[var(--accent-color)]">~{{ totalTokensEstimate }} tok</strong>
          </span>
        </div>
      </template>
    </SubtabBar>

    <!-- Alert Notifications -->
    <div
      v-if="errorMessage"
      class="px-3 py-1.5 bg-[var(--status-error-bg)] border-b border-[var(--status-error-border)] text-[var(--status-error-color)] text-[10px] flex items-center justify-between gap-2 shrink-0"
    >
      <div class="flex items-center gap-1.5 min-w-0">
        <AlertCircle :size="12" class="shrink-0" />
        <span class="truncate">{{ errorMessage }}</span>
      </div>
      <button type="button" @click="errorMessage = null" class="hover:opacity-80 cursor-pointer shrink-0">
        <X :size="12" />
      </button>
    </div>

    <div
      v-if="successMessage"
      class="px-3 py-1.5 bg-[var(--status-ok-bg)] border-b border-[var(--status-ok-border)] text-[var(--status-ok-color)] text-[10px] flex items-center justify-between gap-2 shrink-0"
    >
      <div class="flex items-center gap-1.5 min-w-0">
        <Check :size="12" class="shrink-0" />
        <span class="truncate">{{ successMessage }}</span>
      </div>
      <button type="button" @click="successMessage = null" class="hover:opacity-80 cursor-pointer shrink-0">
        <X :size="12" />
      </button>
    </div>

    <!-- Main Tab Viewport (Split with Preview if active) -->
    <div class="flex-1 min-h-0 flex flex-col lg:flex-row overflow-hidden">
      <!-- Primary Content Column -->
      <div class="flex-1 min-h-0 overflow-y-auto p-3 flex flex-col">
        <!-- ── SUB-TAB 1: INSTALLED SKILLS ──────────────────────────────── -->
        <template v-if="activeSubTab === 'installed'">
          <div v-if="isLoadingInstalled" class="flex-1 flex flex-col items-center justify-center gap-2 text-[var(--dim-color)]">
            <Loader2 :size="20" class="animate-spin text-[var(--accent-color)]" />
            <span class="text-[10px] uppercase tracking-wider">Loading project skills...</span>
          </div>

          <AppEmptyState
            v-else-if="filteredInstalledSkills.length === 0"
            :icon="BrainCircuit"
            :title="filterQuery ? 'No matching skills' : 'No skills installed'"
          >
            <template #description>
              {{
                filterQuery
                  ? `No installed skills match "${filterQuery}".`
                  : 'Use Explore to install skills from documentation URLs or the SkillsMP registry.'
              }}
            </template>
          </AppEmptyState>

          <!-- GRID MODE (Default: compact square cards) -->
          <div
            v-else-if="viewMode === 'grid'"
            class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-2.5"
          >
            <div
              v-for="skill in filteredInstalledSkills"
              :key="skill.name"
              class="border p-3 flex flex-col justify-between min-h-[168px] transition-colors"
              :class="
                skill.is_enabled
                  ? 'border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)]/60'
                  : 'border-[var(--border-color)]/50 ui-surface-list opacity-70'
              "
            >
              <!-- Card Header -->
              <div class="border-b border-[var(--border-color)] pb-2 flex items-center justify-between gap-2 min-w-0">
                <span
                  class="text-[10px] font-bold uppercase tracking-wider truncate"
                  :class="skill.is_enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]'"
                >
                  {{ skill.name }}
                </span>

                <span
                  v-if="skill.is_platform_base"
                  class="text-[8px] px-1.5 py-0.5 border border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)] flex items-center gap-1 font-bold uppercase shrink-0"
                >
                  <ShieldCheck :size="10" />
                  <span>BASE</span>
                </span>
                <span
                  v-else-if="getAssignedAgentsForSkill(skill.name).length > 0"
                  class="text-[8px] px-1.5 py-0.5 border border-[var(--accent-color)]/40 text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold uppercase truncate max-w-[110px] shrink-0"
                >
                  {{ getAssignedAgentsForSkill(skill.name).map((id) => `@${id}`).join(' ') }}
                </span>
                <span
                  v-else
                  class="text-[8px] px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] uppercase font-bold shrink-0"
                >
                  ALL
                </span>
              </div>

              <!-- Card Body -->
              <p class="flex-1 py-2 text-[10px] text-[var(--dim-color)] line-clamp-3 leading-relaxed">
                {{ skill.description || 'No description provided in frontmatter.' }}
              </p>

              <!-- Card Footer -->
              <div class="pt-2 border-t border-[var(--border-color)]/40 flex items-center justify-between gap-2 shrink-0">
                <span class="text-[9px] text-[var(--dim-color)] tabular-nums">
                  ~{{ skill.token_estimate_count }} tok
                </span>

                <div class="flex items-center gap-1.5">
                  <AppTooltip text="Inspect SKILL.md">
                    <button
                      type="button"
                      @click="handleInspectSkill(skill)"
                      class="ui-icon-btn"
                      :class="previewSkillName === skill.name ? '!border-[var(--accent-color)] !text-[var(--accent-color)]' : ''"
                    >
                      <Eye :size="12" />
                    </button>
                  </AppTooltip>

                  <AppTooltip v-if="!skill.is_platform_base" text="Remove skill">
                    <button
                      type="button"
                      @click="handleRemoveSkill(skill)"
                      class="ui-icon-btn !border-[var(--status-error-border)] !text-[var(--status-error-color)] hover:!border-[var(--status-error-color)]"
                    >
                      <Trash2 :size="12" />
                    </button>
                  </AppTooltip>

                  <AppTooltip
                    :text="
                      skill.is_platform_base
                        ? 'Platform Base skill cannot be disabled'
                        : skill.is_enabled
                          ? 'Disable (hide from runners)'
                          : 'Enable (expose to runners)'
                    "
                  >
                    <div class="flex items-center">
                      <AppToggle
                        :model-value="skill.is_enabled"
                        :disabled="skill.is_platform_base"
                        @update:model-value="handleToggleSkill(skill)"
                      />
                    </div>
                  </AppTooltip>
                </div>
              </div>
            </div>
          </div>

          <!-- LIST MODE -->
          <div v-else class="flex flex-col gap-2">
            <div
              v-for="skill in filteredInstalledSkills"
              :key="skill.name"
              class="border px-3 py-2 flex items-center justify-between gap-3 transition-colors"
              :class="
                skill.is_enabled
                  ? 'border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)]/60'
                  : 'border-[var(--border-color)]/50 ui-surface-list opacity-70'
              "
            >
              <div class="flex flex-col gap-0.5 min-w-0 flex-1">
                <div class="flex items-center gap-2 min-w-0">
                  <span
                    class="font-bold text-[10px] uppercase tracking-wider truncate"
                    :class="skill.is_enabled ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]'"
                  >
                    {{ skill.name }}
                  </span>

                  <span
                    v-if="skill.is_platform_base"
                    class="text-[8px] px-1.5 py-0.5 border border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)] flex items-center gap-1 font-bold uppercase shrink-0"
                  >
                    <ShieldCheck :size="10" />
                    <span>UNIVERSAL</span>
                  </span>
                  <span
                    v-else-if="getAssignedAgentsForSkill(skill.name).length > 0"
                    class="text-[8px] px-1.5 py-0.5 border border-[var(--accent-color)]/40 text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold uppercase truncate shrink-0"
                  >
                    {{ getAssignedAgentsForSkill(skill.name).map((id) => `@${id}`).join(' ') }}
                  </span>
                  <span
                    v-else
                    class="text-[8px] px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] uppercase font-bold shrink-0"
                  >
                    GENERAL
                  </span>

                  <span class="text-[9px] text-[var(--dim-color)] tabular-nums shrink-0">
                    ~{{ skill.token_estimate_count }} tok
                  </span>
                </div>

                <p class="text-[10px] text-[var(--dim-color)] truncate">
                  {{ skill.description || 'No description provided in frontmatter.' }}
                </p>
              </div>

              <div class="flex items-center gap-1.5 shrink-0">
                <AppTooltip text="Inspect SKILL.md">
                  <button
                    type="button"
                    @click="handleInspectSkill(skill)"
                    class="ui-icon-btn"
                    :class="previewSkillName === skill.name ? '!border-[var(--accent-color)] !text-[var(--accent-color)]' : ''"
                  >
                    <Eye :size="12" />
                  </button>
                </AppTooltip>

                <AppTooltip v-if="!skill.is_platform_base" text="Remove skill">
                  <button
                    type="button"
                    @click="handleRemoveSkill(skill)"
                    class="ui-icon-btn !border-[var(--status-error-border)] !text-[var(--status-error-color)] hover:!border-[var(--status-error-color)]"
                  >
                    <Trash2 :size="12" />
                  </button>
                </AppTooltip>

                <AppTooltip
                  :text="
                    skill.is_platform_base
                      ? 'Platform Base skill cannot be disabled'
                      : skill.is_enabled
                        ? 'Disable (hide from runners)'
                        : 'Enable (expose to runners)'
                  "
                >
                  <div class="flex items-center">
                    <AppToggle
                      :model-value="skill.is_enabled"
                      :disabled="skill.is_platform_base"
                      @update:model-value="handleToggleSkill(skill)"
                    />
                  </div>
                </AppTooltip>
              </div>
            </div>
          </div>
        </template>

        <!-- ── SUB-TAB 2: EXPLORE (Unified Search & URL Ingestion) ── -->
        <div v-else class="flex-1 min-h-0 flex flex-col gap-3">
          <!-- Quick Suggestions Strip -->
          <div class="flex items-center gap-1.5 overflow-x-auto scrollbar-none shrink-0 text-[9px] text-[var(--dim-color)]">
            <span class="uppercase font-bold shrink-0">Suggestions:</span>
            <button
              v-for="tag in quickSearchTags"
              :key="tag"
              type="button"
              @click="handleSearch(tag)"
              class="h-6 px-2 border border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer shrink-0"
            >
              {{ tag }}
            </button>
          </div>

          <!-- Loading State -->
          <div v-if="isSearching" class="flex-1 flex flex-col items-center justify-center gap-2 text-[var(--dim-color)]">
            <Loader2 :size="20" class="animate-spin text-[var(--accent-color)]" />
            <span class="text-[10px] uppercase tracking-wider">Querying SkillsMP registry...</span>
          </div>

          <!-- Empty State (No Results) -->
          <AppEmptyState
            v-else-if="hasSearched && searchResults.length === 0"
            :icon="Search"
            title="No skills found"
          >
            <template #description>
              No community skills matched "{{ searchQuery }}". Try another library or API name.
            </template>
          </AppEmptyState>

          <!-- Results: GRID MODE (Default) -->
          <div
            v-else-if="searchResults.length > 0 && viewMode === 'grid'"
            class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-2.5"
          >
            <div
              v-for="item in searchResults"
              :key="item.id"
              class="border border-[var(--border-color)] ui-surface-card p-3 flex flex-col justify-between min-h-[168px] hover:border-[var(--accent-color)]/60 transition-colors"
            >
              <!-- Card Header -->
              <div class="border-b border-[var(--border-color)] pb-2 flex items-center justify-between gap-2 min-w-0">
                <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate">
                  {{ item.name }}
                </span>
                <span
                  v-if="item.stars_count"
                  class="text-[8px] text-[var(--status-warn-color)] border border-[var(--status-warn-border)] px-1 py-0.5 bg-[var(--status-warn-bg)] flex items-center gap-0.5 tabular-nums shrink-0"
                >
                  <Star :size="9" />
                  <span>{{ item.stars_count.toLocaleString() }}</span>
                </span>
              </div>

              <!-- Card Body -->
              <p class="flex-1 py-2 text-[10px] text-[var(--dim-color)] line-clamp-3 leading-relaxed">
                {{ item.description || 'No description provided.' }}
              </p>

              <!-- Card Footer -->
              <div class="pt-2 border-t border-[var(--border-color)]/40 flex items-center justify-between gap-2 shrink-0">
                <a
                  :href="item.github_url"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-[9px] text-[var(--dim-color)] hover:text-[var(--accent-color)] flex items-center gap-1 truncate"
                >
                  <span class="truncate">@{{ item.author }}</span>
                  <ExternalLink :size="9" class="shrink-0" />
                </a>

                <button
                  v-if="isSkillInstalled(item.name)"
                  type="button"
                  disabled
                  class="h-6 px-2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)] text-[9px] font-bold uppercase flex items-center gap-1 opacity-80 shrink-0"
                >
                  <Check :size="10" />
                  <span>Installed</span>
                </button>
                <button
                  v-else
                  type="button"
                  @click="handleInstallSkill(item)"
                  :disabled="installingSkillId === item.id"
                  class="h-6 px-2 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black text-[9px] font-bold uppercase transition-colors flex items-center gap-1 cursor-pointer disabled:opacity-40 shrink-0"
                >
                  <Loader2 v-if="installingSkillId === item.id" :size="10" class="animate-spin" />
                  <Download v-else :size="10" />
                  <span>Install</span>
                </button>
              </div>
            </div>
          </div>

          <!-- Results: LIST MODE -->
          <div v-else-if="searchResults.length > 0" class="flex flex-col gap-2">
            <div
              v-for="item in searchResults"
              :key="item.id"
              class="border border-[var(--border-color)] ui-surface-card px-3 py-2 flex items-center justify-between gap-3 hover:border-[var(--accent-color)]/60 transition-colors"
            >
              <div class="flex flex-col gap-0.5 min-w-0 flex-1">
                <div class="flex items-center gap-2 min-w-0">
                  <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)] truncate">
                    {{ item.name }}
                  </span>
                  <a
                    :href="item.github_url"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="text-[9px] text-[var(--dim-color)] hover:text-[var(--accent-color)] flex items-center gap-1 shrink-0"
                  >
                    <span>@{{ item.author }}</span>
                    <ExternalLink :size="9" />
                  </a>
                  <span
                    v-if="item.stars_count"
                    class="text-[8px] text-[var(--status-warn-color)] border border-[var(--status-warn-border)] px-1 py-0.5 bg-[var(--status-warn-bg)] flex items-center gap-0.5 tabular-nums shrink-0"
                  >
                    <Star :size="9" />
                    <span>{{ item.stars_count.toLocaleString() }}</span>
                  </span>
                </div>
                <p class="text-[10px] text-[var(--dim-color)] truncate">
                  {{ item.description || 'No description provided.' }}
                </p>
              </div>

              <div class="shrink-0">
                <button
                  v-if="isSkillInstalled(item.name)"
                  type="button"
                  disabled
                  class="h-6 px-2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)] text-[9px] font-bold uppercase flex items-center gap-1 opacity-80"
                >
                  <Check :size="10" />
                  <span>Installed</span>
                </button>
                <button
                  v-else
                  type="button"
                  @click="handleInstallSkill(item)"
                  :disabled="installingSkillId === item.id"
                  class="h-6 px-2 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black text-[9px] font-bold uppercase transition-colors flex items-center gap-1 cursor-pointer disabled:opacity-40"
                >
                  <Loader2 v-if="installingSkillId === item.id" :size="10" class="animate-spin" />
                  <Download v-else :size="10" />
                  <span>Install</span>
                </button>
              </div>
            </div>
          </div>

          <!-- Initial Empty State -->
          <AppEmptyState
            v-else
            :icon="Sparkles"
            title="Discover community skills"
          >
            <template #description>
              Search the SkillsMP registry above or pick a suggestion to install on-demand skills.
            </template>
          </AppEmptyState>
        </div>
      </div>

      <!-- Right Column: SKILL.md Inspector Split Panel -->
      <div
        v-if="previewSkillName"
        class="w-full lg:w-96 shrink-0 border-t lg:border-t-0 lg:border-l border-[var(--border-color)] ui-surface-list flex flex-col min-h-0 overflow-hidden"
      >
        <div class="h-9 px-3 border-b border-[var(--border-color)] flex items-center justify-between gap-2 ui-surface-title shrink-0">
          <div class="flex items-center gap-1.5 min-w-0">
            <Eye :size="12" class="text-[var(--accent-color)] shrink-0" />
            <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate">
              {{ previewSkillName }} / SKILL.md
            </span>
          </div>
          <AppTooltip text="Close preview">
            <button
              type="button"
              @click="previewSkillName = null"
              class="ui-icon-btn shrink-0"
            >
              <X :size="12" />
            </button>
          </AppTooltip>
        </div>

        <div v-if="isLoadingPreview" class="flex-1 flex items-center justify-center gap-2 text-[10px] text-[var(--dim-color)]">
          <Loader2 :size="14" class="animate-spin text-[var(--accent-color)]" />
          <span>Loading content...</span>
        </div>

        <pre
          v-else
          class="flex-1 p-3 overflow-y-auto text-[11px] leading-relaxed text-[var(--fg-color)]/90 whitespace-pre-wrap font-mono select-text"
        >{{ previewContent }}</pre>
      </div>
    </div>
  </div>
</template>
