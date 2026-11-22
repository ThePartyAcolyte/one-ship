<script setup lang="ts">
import { useAccountStore } from '../../stores/useAccountStore';
import AppTooltip from '../ui/AppTooltip.vue';

const accountStore = useAccountStore();

function handleClose() {
  accountStore.showSwitchWarningModal = false;
  accountStore.pendingSwitchTargetId = null;
}

function handleSwitchKeepActive() {
  accountStore.confirmAccountSwitch(false);
}

function handleSwitchRestartActive() {
  accountStore.confirmAccountSwitch(true);
}
</script>

<template>
  <div v-if="accountStore.showSwitchWarningModal" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4">
    <div class="ui-surface-modal border border-[var(--border-color)] p-6 max-w-lg w-full flex flex-col gap-4 shadow-2xl">
      <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
        <h3 class="text-[var(--status-warn-color)] font-bold uppercase tracking-widest text-xs">
          Active Sessions In Progress
        </h3>
        <span class="text-[10px] text-[var(--dim-color)] font-mono">AGY // PRESENCE GUARD</span>
      </div>

      <p class="text-xs text-[var(--fg-color)] leading-relaxed">
        One or more agents currently have active running sessions. You can switch the global identity while keeping active chats running as they are, or interrupt and restart them under the new account.
      </p>

      <div v-if="accountStore.activeSessionsWarning.length > 0" class="bg-[var(--ui-bg-card)] p-2.5 border border-[var(--border-color)] flex flex-col gap-1.5 text-[11px] font-mono">
        <div class="text-[9px] uppercase tracking-wider text-[var(--dim-color)]">Active Agents Detected:</div>
        <div 
          v-for="s in accountStore.activeSessionsWarning" 
          :key="s.pid" 
          class="flex items-center justify-between text-[var(--fg-color)]"
        >
          <span>PID: {{ s.pid }} {{ s.agent_key ? `[${s.agent_key}]` : '' }}</span>
          <span class="text-[10px] text-[var(--dim-color)] truncate max-w-[160px]">{{ s.conversation_id || 'no-conversation' }}</span>
        </div>
      </div>

      <div class="flex flex-wrap justify-end gap-2 mt-2">
        <AppTooltip text="Keep existing account identity and abort switch">
          <button 
            @click="handleClose" 
            class="px-3 py-1.5 text-xs font-bold uppercase border border-[var(--border-color)] hover:bg-[var(--fg-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer"
          >
            Cancel
          </button>
        </AppTooltip>

        <AppTooltip text="Switch global identity for future chats; keep active conversations running untouched">
          <button 
            @click="handleSwitchKeepActive" 
            class="px-3 py-1.5 text-xs font-bold uppercase border border-[var(--status-info-border)] text-[var(--status-info-color)] hover:bg-[var(--status-info-color)] hover:text-black transition-colors cursor-pointer"
          >
            Keep Active Chats
          </button>
        </AppTooltip>

        <AppTooltip text="Interrupt running agents and reload them immediately with new credentials">
          <button 
            @click="handleSwitchRestartActive" 
            class="px-3 py-1.5 text-xs font-bold uppercase border border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:bg-[var(--status-warn-color)] hover:text-black transition-colors cursor-pointer"
          >
            Switch & Restart Chats
          </button>
        </AppTooltip>
      </div>
    </div>
  </div>
</template>
