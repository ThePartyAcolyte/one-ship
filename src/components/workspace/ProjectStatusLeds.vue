<script setup lang="ts">
import { computed } from 'vue';
import { useCoreStore } from '../../stores/coreStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { AppTooltip } from '../ui';

const props = withDefaults(defineProps<{
  projectId: number;
  projectPath?: string;
  orientation?: 'horizontal' | 'vertical';
  showSocketsWhenIdle?: boolean;
}>(), {
  orientation: 'horizontal',
  showSocketsWhenIdle: false,
});

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();

const runningServices = computed(() => {
  return coreStore.runningServices.filter(s => s.startsWith(`${props.projectId}:`));
});

const agentState = computed(() => coreStore.agentStates[props.projectId]);

const chatStatus = computed(() => {
  const agents = agentState.value?.agents || [];

  // 1. Pending human approval (Highest urgency: Amber pulse)
  for (const ag of agents) {
    const conv = dialogueStore.getConversation(ag.id, props.projectPath);
    if (conv?.messages && conv.messages.length > 0) {
      const last = conv.messages[conv.messages.length - 1];
      if (last.role === 'agent' && last.approval?.status === 'pending') {
        return {
          type: 'approval',
          tooltip: `Agent waiting for user approval (${ag.id})`,
          dotClass: 'bg-[var(--status-warn-color)] animate-pulse shadow-[0_0_6px_var(--status-warn-color)]',
        };
      }
    }
  }

  // 2. Agent execution error / crash (Rose pulse)
  for (const ag of agents) {
    const err = props.projectPath ? dialogueStore.getAgentError(ag.id, props.projectPath) : null;
    if (err) {
      return {
        type: 'error',
        tooltip: `Agent execution error (${ag.id}): ${err.type}`,
        dotClass: 'bg-[var(--status-error-color)] animate-pulse shadow-[0_0_6px_var(--status-error-color)]',
      };
    }
  }

  // 3. Agent actively executing turn (Emerald ping)
  for (const ag of agents) {
    const isRunning = ag.status === 'working' || ag.status === 'running';
    const liveStep = dialogueStore.getActiveStep(ag.id, props.projectPath);
    const isStepActive = liveStep?.state === 'ACTIVE';
    if (isRunning || isStepActive) {
      return {
        type: 'working',
        tooltip: `Agent working (${ag.id})`,
        dotClass: 'bg-[var(--status-ok-color)] animate-ping shadow-[0_0_6px_var(--status-ok-color)]',
      };
    }
  }

  // 4. Unread responses ready to review (Cyan solid)
  let totalUnread = 0;
  for (const ag of agents) {
    const conv = dialogueStore.getConversation(ag.id, props.projectPath);
    if (conv && (conv.unread || 0) > 0) {
      totalUnread += conv.unread;
    }
  }
  if (totalUnread > 0) {
    return {
      type: 'unread',
      tooltip: `${totalUnread} unread agent response(s)`,
      dotClass: 'bg-[var(--accent-color)] shadow-[0_0_6px_color-mix(in_srgb,var(--accent-color)_70%,transparent)]',
    };
  }

  return null;
});

const terminalStatus = computed(() => {
  const count = runningServices.value.length;
  if (count > 0) {
    return {
      tooltip: `${count} background service(s) active`,
      dotClass: 'bg-[var(--status-ok-color)] shadow-[0_0_6px_var(--status-ok-color)]',
    };
  }
  return null;
});
</script>

<template>
  <div 
    class="flex items-center gap-1 shrink-0 select-none"
    :class="orientation === 'vertical' ? 'flex-col' : 'flex-row'"
  >
    <!-- ── 1. AGENT / CHAT LED INDICATOR ──────────────────────── -->
    <template v-if="chatStatus">
      <AppTooltip :text="chatStatus.tooltip" :position="orientation === 'vertical' ? 'right' : 'top'">
        <span 
          class="w-1.5 h-1.5 rounded-full shrink-0 transition-all duration-200"
          :class="chatStatus.dotClass"
        />
      </AppTooltip>
    </template>
    <template v-else-if="showSocketsWhenIdle">
      <AppTooltip text="Agent: Idle" :position="orientation === 'vertical' ? 'right' : 'top'">
        <span class="w-1.5 h-1.5 rounded-full shrink-0 bg-white/10 border border-white/10" />
      </AppTooltip>
    </template>

    <!-- ── 2. TERMINAL / BACKGROUND SERVICE LED INDICATOR ─────── -->
    <template v-if="terminalStatus">
      <AppTooltip :text="terminalStatus.tooltip" :position="orientation === 'vertical' ? 'right' : 'top'">
        <span 
          class="w-1.5 h-1.5 rounded-full shrink-0 transition-all duration-200"
          :class="terminalStatus.dotClass"
        />
      </AppTooltip>
    </template>
    <template v-else-if="showSocketsWhenIdle">
      <AppTooltip text="Services: Stopped" :position="orientation === 'vertical' ? 'right' : 'top'">
        <span class="w-1.5 h-1.5 rounded-full shrink-0 bg-white/10 border border-white/10" />
      </AppTooltip>
    </template>
  </div>
</template>
