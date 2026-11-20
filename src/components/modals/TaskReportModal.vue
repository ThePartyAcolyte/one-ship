<script setup lang="ts">
import { ref, watch } from 'vue';
import { Terminal, X, Copy, Check, Clock } from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { formatDurationMs as formatDuration, formatIsoTimestamp as formatTimestamp } from '../../utils/format';
import { AppTooltip } from '../ui';
import type { ProjectRecord, TaskExecutionRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord | null;
  initialExecId?: number | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const executions = ref<TaskExecutionRecord[]>([]);
const selectedExec = ref<TaskExecutionRecord | null>(null);
const isLoading = ref(false);
const activeTab = ref<'summary' | 'logs'>('logs');
const copied = ref(false);

async function loadExecutions() {
  if (!props.project?.id) return;
  isLoading.value = true;
  try {
    const list = await daemonCall('get_task_executions', {
      projectId: props.project.id,
      limitCount: 50,
    });
    executions.value = list || [];
    
    if (props.initialExecId) {
      const match = executions.value.find(e => e.id === props.initialExecId);
      if (match) {
        await selectExecution(match.id);
      } else if (executions.value.length > 0) {
        await selectExecution(executions.value[0].id);
      }
    } else if (executions.value.length > 0) {
      await selectExecution(executions.value[0].id);
    }
  } catch (err) {
    console.error('Failed to load executions:', err);
  } finally {
    isLoading.value = false;
  }
}

async function selectExecution(id: number) {
  try {
    const report = await daemonCall('get_task_report', { executionId: id });
    selectedExec.value = report;
  } catch (err) {
    console.error('Failed to load task report:', err);
  }
}

function copyLogs() {
  if (!selectedExec.value?.stdout_log) return;
  navigator.clipboard.writeText(selectedExec.value.stdout_log);
  copied.value = true;
  setTimeout(() => {
    copied.value = false;
  }, 2000);
}

watch(() => props.project, () => {
  loadExecutions();
}, { immediate: true });
</script>

<template>
  <div 
    class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
    @click.self="emit('close')"
  >
    <div class="bg-[var(--bg-color)] border border-[var(--border-color)] w-full max-w-4xl max-h-[85vh] flex flex-col shadow-2xl text-[var(--fg-color)]">
      <!-- Modal Header -->
      <div class="border-b border-[var(--border-color)] px-4 py-2.5 flex items-center justify-between bg-black/40">
        <div class="flex items-center gap-2 text-xs">
          <Terminal :size="14" class="text-[var(--accent-color)]" />
          <span class="font-bold tracking-wider">EXECUTION HISTORY</span>
          <span class="text-[var(--dim-color)]">// {{ project?.name }}</span>
        </div>
        <AppTooltip text="Close" position="bottom">
          <button 
            @click="emit('close')"
            class="text-[var(--dim-color)] hover:text-[var(--accent-color)] p-1 transition-colors cursor-pointer"
          >
            <X :size="14" />
          </button>
        </AppTooltip>
      </div>

      <!-- Main Body -->
      <div class="flex-1 flex min-h-[420px] max-h-[65vh] overflow-hidden">
        <!-- Sidebar: Executions List -->
        <div class="w-64 border-r border-[var(--border-color)] overflow-y-auto flex flex-col bg-black/20 shrink-0">
          <div class="px-3 py-1.5 border-b border-[var(--border-color)]/40 text-[9px] text-[var(--dim-color)] uppercase tracking-wider">
            Past Runs ({{ executions.length }})
          </div>

          <div v-if="isLoading && executions.length === 0" class="p-4 text-xs text-[var(--dim-color)] opacity-60 text-center">
            Loading history...
          </div>

          <div v-else-if="executions.length === 0" class="p-4 text-xs text-[var(--dim-color)] opacity-50 text-center italic">
            No executions recorded yet.
          </div>

          <div 
            v-for="exec in executions" 
            :key="exec.id"
            @click="selectExecution(exec.id)"
            class="px-3 py-2 border-b border-[var(--border-color)]/30 cursor-pointer transition-colors text-xs flex flex-col gap-1"
            :class="selectedExec?.id === exec.id ? 'bg-[var(--accent-color)]/10 border-l-2 border-l-[var(--accent-color)]' : 'hover:bg-black/30'"
          >
            <div class="flex items-center justify-between">
              <span class="font-bold text-[11px] truncate">{{ exec.task_id }}</span>
              <span 
                class="text-[9px] px-1 py-0.2 border uppercase"
                :class="exec.status === 'completed' ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : (exec.status === 'running' ? 'border-[var(--status-info-border)] text-[var(--status-info-color)] animate-pulse' : 'border-[var(--status-error-border)] text-[var(--status-error-color)]')"
              >
                {{ exec.status }}
              </span>
            </div>
            <div class="flex items-center justify-between text-[10px] text-[var(--dim-color)]">
              <span>{{ formatTimestamp(exec.started_at) }}</span>
              <span v-if="exec.duration_ms">{{ formatDuration(exec.duration_ms) }}</span>
            </div>
          </div>
        </div>

        <!-- Detail Area -->
        <div class="flex-1 flex flex-col min-w-0 bg-black/10">
          <template v-if="selectedExec">
            <!-- Detail Header Bar -->
            <div class="p-3 border-b border-[var(--border-color)] flex flex-col gap-2 bg-black/30">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <span class="text-xs font-bold text-[var(--accent-color)]">{{ selectedExec.task_id }}</span>
                  <span 
                    class="text-[9px] px-1.5 py-0.5 border uppercase font-mono"
                    :class="selectedExec.status === 'completed' ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--status-error-border)] text-[var(--status-error-color)]'"
                  >
                    {{ selectedExec.status }} [Code: {{ selectedExec.exit_code }}]
                  </span>
                  <span class="text-[10px] text-[var(--dim-color)] flex items-center gap-1">
                    <Clock :size="10" /> {{ formatDuration(selectedExec.duration_ms) }}
                  </span>
                </div>

                <!-- Tabs: Logs vs Summary -->
                <div class="flex items-center gap-1 border border-[var(--border-color)] p-0.5 text-[10px]">
                  <button
                    @click="activeTab = 'logs'"
                    class="px-2 py-0.5 transition-colors cursor-pointer"
                    :class="activeTab === 'logs' ? 'bg-[var(--accent-color)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  >
                    RAW LOG
                  </button>
                  <button
                    @click="activeTab = 'summary'"
                    class="px-2 py-0.5 transition-colors cursor-pointer"
                    :class="activeTab === 'summary' ? 'bg-[var(--accent-color)] text-[var(--bg-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  >
                    SUMMARY
                  </button>
                </div>
              </div>

              <!-- Executed Command -->
              <div class="text-[10px] bg-black/50 border border-[var(--border-color)]/60 px-2 py-1 flex items-center justify-between text-[var(--dim-color)]">
                <span class="truncate font-mono">$ {{ selectedExec.command }}</span>
                <button
                  v-if="activeTab === 'logs'"
                  @click="copyLogs"
                  class="shrink-0 ml-2 hover:text-[var(--accent-color)] transition-colors inline-flex items-center gap-1 text-[9px] cursor-pointer"
                >
                  <Check v-if="copied" :size="11" class="text-[var(--status-ok-color)]" />
                  <Copy v-else :size="11" />
                  <span>{{ copied ? 'COPIED' : 'COPY' }}</span>
                </button>
              </div>
            </div>

            <!-- Detail Content -->
            <div class="flex-1 p-3 overflow-y-auto min-h-0">
              <!-- Logs View -->
              <div v-if="activeTab === 'logs'" class="h-full">
                <pre v-if="selectedExec.stdout_log" class="text-[10px] font-mono whitespace-pre-wrap select-text text-[var(--fg-color)] leading-relaxed break-all bg-black/40 p-2.5 border border-[var(--border-color)]/40 min-h-full">{{ selectedExec.stdout_log }}</pre>
                <div v-else class="text-xs text-[var(--dim-color)] opacity-50 italic text-center p-8">
                  No standard output was captured for this execution.
                </div>
              </div>

              <!-- Summary Markdown View -->
              <div v-else class="text-xs space-y-3 p-2 select-text">
                <div v-if="selectedExec.summary" class="border-l-2 border-[var(--accent-color)] pl-2 text-[11px] font-bold">
                  {{ selectedExec.summary }}
                </div>
                <div v-if="selectedExec.details_markdown" class="whitespace-pre-wrap text-[11px] leading-relaxed text-[var(--fg-color)] font-mono">
                  {{ selectedExec.details_markdown }}
                </div>
              </div>
            </div>
          </template>

          <div v-else class="flex-1 flex items-center justify-center text-xs text-[var(--dim-color)] opacity-50 italic">
            Select an execution from the list to view its report.
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
