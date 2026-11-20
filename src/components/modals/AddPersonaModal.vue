<script setup lang="ts">
import { ref, watch, computed, onMounted, onUnmounted } from 'vue';
import { Bot, X, Trash2, Loader2, Sparkles, BrainCircuit, Check } from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { AppSelect, AppInput, AppTextarea, AppToggle, AppFileTree, AppTooltip, AppAgentPortrait } from '../ui';
import type { FileTreeNode } from '../ui';
import type { AgentPersonaInput, ProjectRecord, ProjectSkillRecord } from '../../types';
import { useRunnerCatalog } from '../../utils/runnerCatalog';

export interface AgentPersonaState {
  id: string;
  role?: string;
  runner?: string;
  model?: string;
  effort?: string;
  accept_edits?: boolean;
  status?: string;
  is_internal?: boolean;
  auto_approve?: boolean;
  args?: string[];
  context_docs?: string[];
  skills?: string[];
  system_prompt?: string;
  dna?: string;
}

const props = defineProps<{
  project: ProjectRecord | null | undefined;
  persona?: AgentPersonaState | null;
  isSaving?: boolean;
}>();

const isEditMode = computed(() => !!props.persona);

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', payload: AgentPersonaInput): void;
  (e: 'delete', payload: { agentId: string }): void;
}>();

const agentId = ref('');
const role = ref('');
const runner = ref('');
const model = ref('');
const effort = ref('');
const autoApprove = ref(false);
const acceptEdits = ref(false);
const rawArgs = ref('');
const rawDocs = ref('');
const systemPrompt = ref('');
const errorMsg = ref('');
const confirmingDelete = ref(false);

const availableSkills = ref<ProjectSkillRecord[]>([]);
const selectedSkills = ref<string[]>([]);
const isLoadingSkills = ref(false);

async function loadProjectSkills() {
  if (!props.project?.path) return;
  isLoadingSkills.value = true;
  try {
    const list: ProjectSkillRecord[] = await daemonCall('get_project_skills', { projectPath: props.project.path });
    availableSkills.value = list.filter(s => !s.is_platform_base);
  } catch (err) {
    console.error('[AddPersonaModal] Failed to load project skills:', err);
    availableSkills.value = [];
  } finally {
    isLoadingSkills.value = false;
  }
}

function toggleSelectedSkill(skillName: string) {
  const idx = selectedSkills.value.indexOf(skillName);
  if (idx !== -1) {
    selectedSkills.value.splice(idx, 1);
  } else {
    selectedSkills.value.push(skillName);
  }
}

// Runners, models and efforts come only from the runner manifests.
const runnerCatalog = useRunnerCatalog();
const runnerSchema = computed(() => runnerCatalog.findRunner(runner.value) ?? null);

const fileTree = ref<FileTreeNode[]>([]);
const selectedDocs = ref<string[]>([]);
const showTree = ref(false);
const isLoadingTree = ref(false);
const treeTruncated = ref(false);
const treeError = ref<string | null>(null);

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.project) {
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
  loadRunnerOptions();
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});

async function loadFileTree() {
  if (fileTree.value.length > 0 || !props.project) return;
  isLoadingTree.value = true;
  treeError.value = null;
  try {
    const tree = await daemonCall("get_project_file_tree", { projectId: props.project.id });
    fileTree.value = tree.nodes;
    treeTruncated.value = tree.truncated;
  } catch (e: unknown) {
    fileTree.value = [];
    treeError.value = String(e);
  } finally {
    isLoadingTree.value = false;
  }
}

const runnerOptions = computed(() =>
  runnerCatalog.runners.value.map(r => ({ label: `${r.id} (${r.name})`, value: r.id })),
);

async function loadRunnerOptions() {
  try {
    await runnerCatalog.loadRunnerCatalog();
  } catch (e: unknown) {
    errorMsg.value = `Cannot list runners: ${String(e)}`;
  }
}

// A model always belongs to its runner: changing the runner clears a model it does not declare.
watch(runner, (newRunner) => {
  if (runnerCatalog.runners.value.length > 0 && model.value && !runnerCatalog.findModel(newRunner, model.value)) {
    model.value = '';
  }
});

const modelOptions = computed(() => [
  { label: 'Select a model', value: '' },
  ...runnerCatalog.modelsOf(runner.value).map(m => ({ label: m.label, value: m.id })),
]);

const currentModelObj = computed(() => runnerCatalog.findModel(runner.value, model.value) ?? null);

// '' writes no `effort`, so the agent runs with the model's declared default.
const effortOptions = computed(() => {
  const m = currentModelObj.value;
  if (!m || m.effort_levels.length === 0) return [];
  return [
    { label: `Model default (${m.default_effort})`, value: '' },
    ...m.effort_levels.map(lvl => ({ label: lvl, value: lvl })),
  ];
});

watch(currentModelObj, (newModel) => {
  if (runnerCatalog.runners.value.length > 0 && effort.value && !newModel?.effort_levels.includes(effort.value)) {
    effort.value = '';
  }
});

const dna = ref('');

function randomizeDna() {
  const randInt = (max: number) => Math.floor(Math.random() * max);
  const pad = (n: number) => String(n).padStart(2, '0');
  const baseIdx = randInt(14) + 1;
  const exprIdx = randInt(6);
  const hairIdx = randInt(22);
  const beardIdx = randInt(6);
  const accIdx = randInt(17);
  dna.value = `B${pad(baseIdx)}:E${exprIdx}:H${pad(hairIdx)}:BD${pad(beardIdx)}:A${pad(accIdx)}`;
}

watch(
  [() => props.project, () => props.persona],
  ([_, persona]) => {
    confirmingDelete.value = false;
    errorMsg.value = '';
    showTree.value = false;
    fileTree.value = [];
    treeTruncated.value = false;
    treeError.value = null;

    loadProjectSkills();

    if (persona) {
      agentId.value = persona.id;
      role.value = persona.role || '';
      runner.value = persona.runner || '';
      model.value = persona.model || '';
      effort.value = persona.effort || '';
      autoApprove.value = !!persona.auto_approve;
      acceptEdits.value = !!persona.accept_edits;
      rawArgs.value = (persona.args || []).join(' ');
      rawDocs.value = (persona.context_docs || []).join('\n');
      selectedDocs.value = [...(persona.context_docs || [])];
      selectedSkills.value = persona.skills ? [...persona.skills] : [];
      systemPrompt.value = persona.system_prompt || '';
      dna.value = persona.dna || '';
    } else {
      agentId.value = '';
      role.value = '';
      // No preselected pair: the user picks a runner, then one of its models.
      runner.value = '';
      model.value = '';
      effort.value = '';
      autoApprove.value = false;
      acceptEdits.value = false;
      rawArgs.value = '';
      rawDocs.value = '';
      selectedDocs.value = [];
      selectedSkills.value = [];
      systemPrompt.value = '';
      randomizeDna();
    }
  },
  { immediate: true }
);

// Sync tree selections to the text area
watch(selectedDocs, (newDocs) => {
  rawDocs.value = newDocs.join('\n');
});

function handleDelete() {
  if (!agentId.value) return;
  emit('delete', { agentId: agentId.value });
}

function handleSave() {
  const cleanId = agentId.value.trim().toLowerCase().replace(/\s+/g, '-');
  if (!cleanId) {
    errorMsg.value = 'Agent ID is required';
    return;
  }
  if (!/^[a-z0-9_-]+$/.test(cleanId)) {
    errorMsg.value = 'Agent ID must contain only alphanumeric characters, dashes, or underscores';
    return;
  }
  if (!role.value.trim()) {
    errorMsg.value = 'Role is required';
    return;
  }
  if (!runnerCatalog.isDeclaredPair(runner.value, model.value, effort.value || null)) {
    errorMsg.value = 'Choose a runner and one of its models';
    return;
  }

  const argsList = rawArgs.value
    ? rawArgs.value.split(/[\s,]+/).map(s => s.trim()).filter(Boolean)
    : [];

  const docsList = rawDocs.value
    ? rawDocs.value.split(/[\n,]+/).map(s => s.trim()).filter(Boolean)
    : [];

  emit('save', {
    id: cleanId,
    role: role.value.trim(),
    runner: runner.value,
    model: model.value,
    effort: effort.value || null,
    accept_edits: acceptEdits.value,
    auto_approve: autoApprove.value,
    args: argsList,
    context_docs: docsList,
    skills: [...selectedSkills.value],
    system_prompt: systemPrompt.value.trim() || null,
    dna: dna.value || null,
  });
}
</script>

<template>
  <div v-if="project" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono" @click.self="emit('close')">
    <div class="bg-[var(--bg-color)] border border-[var(--border-color)] p-6 max-w-lg w-full flex flex-col gap-4 shadow-2xl text-xs max-h-[90vh] overflow-y-auto">
      <!-- Modal Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-2">
        <h3 class="font-bold uppercase tracking-widest text-[var(--accent-color)] flex items-center gap-2">
          <Bot :size="16" />
          <span v-if="isEditMode">Edit Persona: {{ agentId }} // {{ project.name }}</span>
          <span v-else>Add Agent Persona // {{ project.name }}</span>
        </h3>
        <button 
          @click="emit('close')" 
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          <X :size="16" />
        </button>
      </div>

      <!-- Form Error Alert -->
      <div v-if="errorMsg" class="p-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-xs">
        {{ errorMsg }}
      </div>

      <!-- Form Fields -->
      <div class="flex flex-col gap-3">
        <!-- Pixel Portrait DNA Card -->
        <div class="flex items-center gap-3 border border-[var(--border-color)] p-2.5 ui-surface-card">
          <AppAgentPortrait :agent-id="agentId" :dna="dna" :size="56" />
          <div class="flex flex-col gap-1 flex-1 min-w-0">
            <div class="flex items-center justify-between">
              <label class="text-[10px] uppercase tracking-wider text-[var(--accent-color)] font-bold">
                Pixel Portrait DNA
              </label>
              <button
                type="button"
                @click="randomizeDna"
                class="px-1.5 py-0.5 border border-[var(--border-color)] text-[9px] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 transition-colors cursor-pointer"
              >
                <Sparkles :size="10" />
                <span>Randomize</span>
              </button>
            </div>
            <AppInput v-model="dna" placeholder="e.g. B01:E0:H06:BD02:A01" />
          </div>
        </div>

        <!-- Identifier -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Agent Identifier (ID) *
          </label>
          <AppInput 
            v-model="agentId"
            :disabled="isEditMode"
            placeholder="e.g. dev, reviewer, tester, bosmang"
          />
          <span class="text-[10px] text-[var(--dim-color)] opacity-60">
            {{ isEditMode ? 'Identifier cannot be modified after creation.' : 'Unique slug for process allocation and MCP task delegation.' }}
          </span>
        </div>

        <!-- Role -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Role / Purpose
          </label>
          <AppInput 
            v-model="role"
            placeholder="e.g. Code Reviewer, Documentation Specialist"
          />
        </div>

        <!-- Runner CLI -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Agent Runner CLI
          </label>
          <AppSelect 
            v-model="runner"
            :options="runnerOptions"
          />
        </div>

        <!-- Model Override -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Model
          </label>
          <AppSelect 
            v-model="model"
            :options="modelOptions"
          />
        </div>

        <!-- Reasoning Effort -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Reasoning Effort
          </label>
          <AppSelect 
            v-if="effortOptions.length > 0"
            v-model="effort"
            :options="effortOptions"
          />
          <AppTooltip v-else text="This model uses fixed reasoning effort (not configurable)." position="top">
            <AppInput 
              :model-value="'Fixed / Non-configurable'" 
              disabled 
              class="opacity-50 cursor-not-allowed" 
            />
          </AppTooltip>
        </div>

        <!-- Auto Approve Toggle -->
        <div class="flex items-center justify-between border border-[var(--border-color)] p-2 bg-black/20">
          <div class="flex flex-col">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)]">
              Auto-Approve Tool Actions
            </span>
            <span class="text-[10px] text-[var(--dim-color)]">
              Execute tool actions without requiring manual confirmation
            </span>
          </div>
          <AppToggle v-model="autoApprove" />
        </div>

        <!-- Auto Accept Edits Toggle -->
        <div v-if="runnerSchema?.flags?.accept_edits" class="flex items-center justify-between border border-[var(--border-color)] p-2 bg-black/20">
          <div class="flex flex-col">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)]">
              Auto-Accept File Edits
            </span>
            <span class="text-[10px] text-[var(--dim-color)]">
              Automatically apply file modifications without prompting
            </span>
          </div>
          <AppToggle v-model="acceptEdits" />
        </div>

        <!-- CLI Extra Arguments -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            Extra CLI Arguments (Space or Comma-separated)
          </label>
          <AppInput 
            v-model="rawArgs"
            placeholder="e.g. --effort high, --debug"
          />
        </div>

        <!-- System Prompt -->
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
            System Prompt (Base Instructions)
          </label>
          <AppTextarea 
            v-model="systemPrompt"
            :rows="3"
            placeholder="e.g. You are a strictly pragmatic code auditor..."
          />
        </div>

        <!-- Context Docs with Visual Tree Explorer -->
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between items-center">
            <label class="text-[10px] uppercase tracking-wider text-[var(--dim-color)] font-bold">
              Context Documents (Auto-Injected at Launch)
            </label>
            <button 
              type="button" 
              @click="showTree = !showTree; if (showTree) loadFileTree();" 
              class="text-[10px] text-[var(--accent-color)] hover:underline cursor-pointer font-mono"
            >
              {{ showTree ? '[Hide File Explorer]' : '[Browse Project Files]' }}
            </button>
          </div>

          <!-- File Tree Selector Component with Loading Indicator -->
          <div v-if="showTree" class="border border-[var(--border-color)] p-2 bg-black/40 max-h-48 overflow-y-auto">
            <div v-if="isLoadingTree" class="flex items-center gap-2 text-[var(--dim-color)] text-xs p-2">
              <Loader2 :size="12" class="animate-spin" />
              <span>Scanning project structure...</span>
            </div>
            <div v-else-if="treeError" class="text-[var(--status-error-color)] text-xs p-2 break-words">
              {{ treeError }}
            </div>
            <template v-else>
              <AppFileTree
                :nodes="fileTree"
                v-model="selectedDocs"
              />
              <AppTooltip v-if="treeTruncated" text="The scan stopped at system.fs_tree_max_depth_count or fs_tree_max_nodes_count; raise them in Settings to list more.">
                <span class="text-[10px] text-[var(--status-warn-color)] p-1">Tree truncated</span>
              </AppTooltip>
            </template>
          </div>

          <AppTextarea 
            v-model="rawDocs"
            :rows="2"
            placeholder="e.g. docs/specs/03-projects/01-project-registry-and-service-orchestration.md, docs/00_manifesto.md"
          />
          <span class="text-[10px] text-[var(--dim-color)] opacity-60">
            Selected files will have their full text prepended into the agent's initial prompt.
          </span>
        </div>

        <!-- Assigned Project Skills (On-Demand & Custom) -->
        <div class="flex flex-col gap-1.5 border border-[var(--border-color)] p-2.5 bg-black/20">
          <div class="flex items-center justify-between">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-1.5">
              <BrainCircuit :size="12" class="text-[var(--accent-color)]" />
              <span>Assigned Project Skills</span>
            </span>
            <span class="text-[9px] text-[var(--dim-color)]">
              Specialized domain skills
            </span>
          </div>

          <div v-if="isLoadingSkills" class="flex items-center gap-2 text-[var(--dim-color)] text-[10px] py-1">
            <Loader2 :size="11" class="animate-spin" />
            <span>Loading project skills...</span>
          </div>

          <div v-else-if="availableSkills.length === 0" class="text-[10px] text-[var(--dim-color)] italic py-1">
            No on-demand skills installed yet. (Add skills via the Skills tab).
          </div>

          <div v-else class="flex flex-wrap gap-1.5 pt-1">
            <button
              v-for="s in availableSkills"
              :key="s.name"
              type="button"
              @click="toggleSelectedSkill(s.name)"
              class="px-2 py-1 border text-[10px] transition-colors cursor-pointer flex items-center gap-1 font-mono"
              :class="selectedSkills.includes(s.name)
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]/40 hover:text-[var(--fg-color)] bg-black/40'"
            >
              <Check v-if="selectedSkills.includes(s.name)" :size="10" />
              <span>{{ s.name }}</span>
            </button>
          </div>

          <span class="text-[9px] text-[var(--dim-color)] opacity-60">
            Platform base skills (core, delegation, devlog, snapshots, etc.) are universal and always active for all agents.
          </span>
        </div>
      </div>

      <!-- Actions Toolbar -->
      <div class="flex justify-between items-center border-t border-[var(--border-color)] pt-3 mt-1">
        <div>
          <!-- Destructive Delete or Revert Button in Edit Mode -->
          <template v-if="isEditMode">
            <div v-if="confirmingDelete" class="flex items-center gap-1.5">
              <button 
                @click="handleDelete" 
                class="px-3 py-1.5 bg-[var(--status-error-color)] text-white font-bold hover:opacity-90 text-xs font-mono transition-colors cursor-pointer"
              >
                {{ persona?.is_internal ? 'Confirm Revert' : 'Confirm Delete' }}
              </button>
              <button 
                @click="confirmingDelete = false" 
                class="px-2 py-1.5 border border-[var(--border-color)] text-[var(--dim-color)] text-xs font-mono hover:text-[var(--fg-color)] cursor-pointer"
              >
                Cancel
              </button>
            </div>
            <button 
              v-else
              @click="confirmingDelete = true" 
              class="px-3 py-1.5 border border-[var(--status-error-border)] text-[var(--status-error-color)] hover:bg-[var(--status-error-bg)] text-xs font-mono transition-colors cursor-pointer flex items-center gap-1.5"
            >
              <Trash2 :size="13" />
              <span>{{ persona?.is_internal ? 'Revert to Base Settings' : 'Delete Persona' }}</span>
            </button>
          </template>
        </div>

        <div class="flex justify-end gap-2">
          <button 
            @click="emit('close')" 
            :disabled="isSaving"
            class="px-4 py-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--fg-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
          <button 
            @click="handleSave" 
            :disabled="isSaving"
            class="px-4 py-1.5 bg-[var(--accent-color)] text-black font-bold hover:brightness-110 transition-all cursor-pointer flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Loader2 v-if="isSaving" :size="12" class="animate-spin" />
            <span>{{ isSaving ? 'Saving...' : (isEditMode ? 'Save Changes' : 'Save Persona') }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
