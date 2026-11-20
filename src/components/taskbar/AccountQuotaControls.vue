<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import { useAccountStore } from '../../stores/useAccountStore';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import AppTooltip from '../ui/AppTooltip.vue';
import AppQuotaGauge from '../ui/AppQuotaGauge.vue';
import AppInput from '../ui/AppInput.vue';
import GeminiIcon from '../ui/GeminiIcon.vue';
import DeepSeekIcon from '../ui/DeepSeekIcon.vue';

const accountStore = useAccountStore();
const metricsStore = useAgentMetricsStore();

interface AccountQuotaSummary {
  gemini5hPct?: number;
  gemini5hResetsAt?: string;
  gemini5hResetEpoch?: number;
  gemini5hLocal?: string;
  gemini7dPct?: number;
  gemini7dResetsAt?: string;
  gemini7dResetEpoch?: number;
  gemini7dLocal?: string;
  other5hPct?: number;
  other5hResetsAt?: string;
  other5hResetEpoch?: number;
  other5hLocal?: string;
  other7dPct?: number;
  other7dResetsAt?: string;
  other7dResetEpoch?: number;
  other7dLocal?: string;
  warmupEnabled?: boolean;
}

function getAccountQuotaSummary(accountId: string): AccountQuotaSummary {
  const accountRecords = metricsStore.accountQuotas.filter(q => q.account_id === accountId);
  if (accountRecords.length === 0) {
    return {};
  }

  // 1. Resolve Gemini Pool record (1P)
  const geminiRec = accountRecords.find(q => {
    const m = (q.model || '').toLowerCase();
    const p = (q.provider || '').toLowerCase();
    return m === 'gemini-pool' || m.includes('gemini') || m.includes('flash') || m.includes('pro') || p.includes('gemini') || p.includes('agy');
  });

  // 2. Resolve Third-Party / External Pool record (3P)
  const otherRec = accountRecords.find(q => {
    const m = (q.model || '').toLowerCase();
    const p = (q.provider || '').toLowerCase();
    return m === '3p-pool' || m.includes('claude') || m.includes('sonnet') || m.includes('opus') || m.includes('gpt') || p.includes('anthropic') || p.includes('openai');
  });

  const warmup = accountRecords[0]?.warmup_enabled ?? true;

  return {
    gemini5hPct: geminiRec?.five_hour_remaining_pct ?? undefined,
    gemini5hResetsAt: geminiRec?.five_hour_resets_at ?? undefined,
    gemini5hResetEpoch: geminiRec?.five_hour_reset_epoch_seconds ?? undefined,
    gemini5hLocal: geminiRec?.five_hour_resets_at_local ?? undefined,
    gemini7dPct: geminiRec?.weekly_remaining_pct ?? undefined,
    gemini7dResetsAt: geminiRec?.weekly_resets_at ?? undefined,
    gemini7dResetEpoch: geminiRec?.weekly_reset_epoch_seconds ?? undefined,
    gemini7dLocal: geminiRec?.weekly_resets_at_local ?? undefined,
    other5hPct: otherRec?.five_hour_remaining_pct ?? undefined,
    other5hResetsAt: otherRec?.five_hour_resets_at ?? undefined,
    other5hResetEpoch: otherRec?.five_hour_reset_epoch_seconds ?? undefined,
    other5hLocal: otherRec?.five_hour_resets_at_local ?? undefined,
    other7dPct: otherRec?.weekly_remaining_pct ?? undefined,
    other7dResetsAt: otherRec?.weekly_resets_at ?? undefined,
    other7dResetEpoch: otherRec?.weekly_reset_epoch_seconds ?? undefined,
    other7dLocal: otherRec?.weekly_resets_at_local ?? undefined,
    warmupEnabled: warmup,
  };
}

function handleKickstart(e: MouseEvent, accountId: string) {
  e.stopPropagation();
  metricsStore.kickstartAccountQuota(accountId);
}

function handleToggleWarmup(e: MouseEvent, accountId: string, currentVal: boolean) {
  e.stopPropagation();
  metricsStore.toggleAccountWarmup(accountId, !currentVal);
}

const isDropdownOpen = ref(false);
const editingAccountId = ref<string | null>(null);
const editingLabel = ref('');

function toggleDropdown() {
  isDropdownOpen.value = !isDropdownOpen.value;
  if (isDropdownOpen.value) {
    isDeepSeekDropdownOpen.value = false;
  }
}

function closeDropdown() {
  isDropdownOpen.value = false;
  editingAccountId.value = null;
}

function handleSwitch(accountId: string) {
  accountStore.requestAccountSwitch(accountId);
  closeDropdown();
}

function handleAddAccount() {
  accountStore.initiateAddAccount();
  closeDropdown();
}

function handleRemoveAccount(e: MouseEvent, accountId: string) {
  e.stopPropagation();
  accountStore.removeAccount(accountId);
}

function startRename(e: MouseEvent, acc: { id: string; label: string }) {
  e.stopPropagation();
  editingAccountId.value = acc.id;
  editingLabel.value = acc.label;
}

function saveRename(e: Event, accountId: string) {
  e.stopPropagation();
  if (editingLabel.value.trim()) {
    accountStore.renameAccount(accountId, editingLabel.value.trim());
  }
  editingAccountId.value = null;
}

const isDeepSeekDropdownOpen = ref(false);
const isRefreshingDeepSeek = ref(false);
const showKeyInput = ref(false);
const newDeepSeekKey = ref('');
const newDeepSeekLabel = ref('');

function toggleDeepSeekDropdown() {
  isDeepSeekDropdownOpen.value = !isDeepSeekDropdownOpen.value;
  if (isDeepSeekDropdownOpen.value) {
    isDropdownOpen.value = false;
    if (!accountStore.deepseekBalance) {
      accountStore.fetchDeepSeekBalance().catch(reportAsyncFailure);
    }
  }
}

function closeDeepSeekDropdown() {
  isDeepSeekDropdownOpen.value = false;
  showKeyInput.value = false;
  newDeepSeekKey.value = '';
  newDeepSeekLabel.value = '';
}

async function refreshDeepSeekBalance() {
  isRefreshingDeepSeek.value = true;
  try {
    await accountStore.fetchDeepSeekBalance();
  } finally {
    isRefreshingDeepSeek.value = false;
  }
}

async function handleSaveDeepSeekKey() {
  if (!newDeepSeekKey.value.trim()) return;
  await accountStore.saveDeepSeekApiKey(newDeepSeekKey.value.trim(), newDeepSeekLabel.value.trim() || undefined);
  showKeyInput.value = false;
  newDeepSeekKey.value = '';
  newDeepSeekLabel.value = '';
}

const deepSeekTotalBalanceNum = computed(() => {
  const info = accountStore.deepseekBalance?.balance_infos?.[0];
  if (!info || !info.total_balance) return null;
  const num = parseFloat(info.total_balance);
  return isNaN(num) ? null : num;
});

const deepSeekBalanceDisplay = computed(() => {
  if (accountStore.isFetchingDeepSeekBalance && !accountStore.deepseekBalance) return '...';
  if (deepSeekTotalBalanceNum.value !== null) {
    return `$${deepSeekTotalBalanceNum.value.toFixed(2)}`;
  }
  if (accountStore.activeDeepseekAccount) {
    return 'NO BAL';
  }
  return 'NO KEY';
});

const deepSeekBalanceClass = computed(() => {
  if (deepSeekTotalBalanceNum.value === null) return 'text-[var(--dim-color)]';
  if (deepSeekTotalBalanceNum.value > 2.0) return 'text-[var(--status-ok-color)]';
  if (deepSeekTotalBalanceNum.value > 0.0) return 'text-[var(--status-warn-color)]';
  return 'text-[var(--status-error-color)]';
});

const deepSeekTotalBalance = computed(() => {
  const info = accountStore.deepseekBalance?.balance_infos?.[0];
  return info ? `$${info.total_balance} ${info.currency}` : '--';
});

const deepSeekToppedUpBalance = computed(() => {
  const info = accountStore.deepseekBalance?.balance_infos?.[0];
  return info ? `$${info.topped_up_balance} ${info.currency}` : '--';
});

const deepSeekGrantedBalance = computed(() => {
  const info = accountStore.deepseekBalance?.balance_infos?.[0];
  return info ? `$${info.granted_balance} ${info.currency}` : '--';
});

const activeGoogleAccountTooltip = computed(() => {
  if (accountStore.isSwitchingAccount) return 'Switching Google Account...';
  if (accountStore.activeGoogleAccount) {
    const label = accountStore.activeGoogleAccount.label || accountStore.activeGoogleAccount.email;
    return `${label} — Google Quota Runway`;
  }
  return 'Google OAuth multi-account manager';
});

function handleDocumentClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest('.account-switcher-container')) {
    closeDropdown();
  }
  if (!target.closest('.deepseek-wallet-container')) {
    closeDeepSeekDropdown();
  }
}

onMounted(() => {
  document.addEventListener('click', handleDocumentClick);
  metricsStore.fetchAccountQuotas().catch(reportAsyncFailure);
  accountStore.fetchDeepSeekBalance().catch(reportAsyncFailure);
});

onUnmounted(() => {
  document.removeEventListener('click', handleDocumentClick);
});
</script>

<template>
  <div class="flex items-center gap-1.5 shrink-0">
    <!-- Google OAuth Multi-Account Switcher -->
    <div class="relative account-switcher-container">
      <AppTooltip :text="activeGoogleAccountTooltip" position="top">
        <button
          @click="toggleDropdown"
          class="h-6 flex items-center gap-1.5 px-2 border border-[var(--border-color)] bg-[var(--bg-color)] hover:border-[var(--accent-color)] text-[10px] font-mono tracking-tight transition-colors cursor-pointer shrink-0 select-none"
        >
          <GeminiIcon :size="14" class="shrink-0" />

          <span v-if="!accountStore.activeGoogleAccount && !accountStore.isSwitchingAccount" class="truncate uppercase text-[var(--dim-color)] text-[9px] leading-none">
            NO AUTH
          </span>
          <span v-else-if="accountStore.isSwitchingAccount" class="truncate uppercase text-[var(--status-warn-color)] text-[9px] animate-pulse leading-none">
            SWITCHING...
          </span>

          <!-- Active Google Account Quota Gauges -->
          <div v-if="accountStore.activeGoogleAccount" class="flex items-center gap-1 shrink-0">
            <AppQuotaGauge
              category="gemini-5h"
              :percentage="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini5hPct"
              :resetsAt="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini5hResetsAt"
              :resetEpochSeconds="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini5hResetEpoch"
              :size="14"
              :width="18"
            />
            <AppQuotaGauge
              category="gemini-7d"
              :percentage="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini7dPct"
              :resetsAt="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini7dResetsAt"
              :resetEpochSeconds="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).gemini7dResetEpoch"
              :size="14"
              :width="18"
            />
            <AppQuotaGauge
              category="other-5h"
              :percentage="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other5hPct"
              :resetsAt="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other5hResetsAt"
              :resetEpochSeconds="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other5hResetEpoch"
              :size="14"
              :width="18"
            />
            <AppQuotaGauge
              category="other-7d"
              :percentage="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other7dPct"
              :resetsAt="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other7dResetsAt"
              :resetEpochSeconds="getAccountQuotaSummary(accountStore.activeGoogleAccount.id).other7dResetEpoch"
              :size="14"
              :width="18"
            />
          </div>

          <!-- Warmup in progress indicator (strictly for active account) -->
          <div
            v-if="metricsStore.isKeyringWarmupInProgress && (!metricsStore.warmupAccountId || metricsStore.warmupAccountId === accountStore.activeGoogleAccount?.id)"
            class="flex items-center gap-1 px-1 h-4 bg-[var(--status-warn-bg)] border border-[var(--status-warn-border)] text-[var(--status-warn-color)] text-[8px] animate-pulse shrink-0 leading-none"
          >
            <span>⚡ WARMING UP</span>
          </div>

          <span class="text-[8px] text-[var(--dim-color)] shrink-0 ml-0.5 leading-none">▲</span>
        </button>
      </AppTooltip>

      <!-- Upward Dropdown Menu -->
      <div
        v-if="isDropdownOpen"
        class="absolute right-0 bottom-full mb-1.5 w-80 sm:w-96 max-w-[calc(100vw-4rem)] bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl z-50 p-1 flex flex-col gap-1 font-mono text-[11px]"
      >
        <div class="px-2 py-1 text-[9px] text-[var(--dim-color)] uppercase tracking-wider border-b border-[var(--border-color)] flex justify-between items-center">
          <span>Google Accounts & Runway</span>
          <span v-if="accountStore.isAddingAccount" class="text-[var(--status-warn-color)] animate-pulse">Auth Open</span>
        </div>

        <div class="flex flex-col max-h-64 overflow-y-auto">
          <div
            v-for="acc in accountStore.googleAccounts"
            :key="acc.id"
            @click="handleSwitch(acc.id)"
            class="flex items-center justify-between px-2 py-1.5 hover:bg-[var(--ui-bg-card)] cursor-pointer group transition-colors"
            :class="{ 'bg-[var(--ui-bg-card)] font-bold text-[var(--accent-color)]': acc.is_active }"
          >
            <div class="flex items-center gap-2 truncate min-w-0 flex-1">
              <span class="text-[10px]" :class="acc.is_active ? 'text-[var(--status-ok-color)]' : 'text-[var(--dim-color)]'">
                {{ acc.is_active ? '●' : '○' }}
              </span>

              <template v-if="editingAccountId === acc.id">
                <AppInput
                  v-model="editingLabel"
                  @click.stop
                  @keyup.enter="saveRename($event, acc.id)"
                  type="text"
                  class="!h-auto !px-1 !py-0.5 text-[10px] uppercase"
                  autofocus
                />
              </template>
              <template v-else>
                <div class="flex flex-col truncate flex-1 min-w-0">
                  <span class="truncate text-[10px] uppercase font-bold text-[var(--fg-color)]">{{ acc.label || acc.email }}</span>
                  <span v-if="acc.label && acc.label !== acc.email" class="text-[8px] uppercase text-[var(--dim-color)] truncate">{{ acc.email }}</span>

                  <!-- Quota Runway Gauges -->
                  <div class="flex items-center gap-2 mt-1 text-[8px] font-mono">
                    <div class="flex items-center gap-1.5">
                      <AppQuotaGauge
                        category="gemini-5h"
                        :percentage="getAccountQuotaSummary(acc.id).gemini5hPct"
                        :resetsAt="getAccountQuotaSummary(acc.id).gemini5hResetsAt"
                        :resetEpochSeconds="getAccountQuotaSummary(acc.id).gemini5hResetEpoch"
                        :size="18"
                        :width="28"
                      />
                      <AppQuotaGauge
                        category="gemini-7d"
                        :percentage="getAccountQuotaSummary(acc.id).gemini7dPct"
                        :resetsAt="getAccountQuotaSummary(acc.id).gemini7dResetsAt"
                        :resetEpochSeconds="getAccountQuotaSummary(acc.id).gemini7dResetEpoch"
                        :size="18"
                        :width="28"
                      />
                      <AppQuotaGauge
                        category="other-5h"
                        :percentage="getAccountQuotaSummary(acc.id).other5hPct"
                        :resetsAt="getAccountQuotaSummary(acc.id).other5hResetsAt"
                        :resetEpochSeconds="getAccountQuotaSummary(acc.id).other5hResetEpoch"
                        :size="18"
                        :width="28"
                      />
                      <AppQuotaGauge
                        category="other-7d"
                        :percentage="getAccountQuotaSummary(acc.id).other7dPct"
                        :resetsAt="getAccountQuotaSummary(acc.id).other7dResetsAt"
                        :resetEpochSeconds="getAccountQuotaSummary(acc.id).other7dResetEpoch"
                        :size="18"
                        :width="28"
                      />
                    </div>

                    <AppTooltip :text="getAccountQuotaSummary(acc.id).warmupEnabled ? 'Auto-warmup active: Click to disable' : 'Auto-warmup disabled: Click to enable'">
                      <button
                        @click="handleToggleWarmup($event, acc.id, !!getAccountQuotaSummary(acc.id).warmupEnabled)"
                        class="text-[8px] ml-0.5 uppercase tracking-tight font-bold cursor-pointer transition-colors"
                        :class="getAccountQuotaSummary(acc.id).warmupEnabled ? 'text-[var(--status-info-color)] hover:opacity-80' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                      >
                        ● {{ getAccountQuotaSummary(acc.id).warmupEnabled ? 'WARM' : 'COLD' }}
                      </button>
                    </AppTooltip>
                  </div>
                </div>
              </template>
            </div>

            <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity ml-2 shrink-0">
              <template v-if="editingAccountId === acc.id">
                <button @click="saveRename($event, acc.id)" class="text-[9px] hover:text-[var(--accent-color)] px-1">✓</button>
              </template>
              <template v-else>
                <AppTooltip :text="metricsStore.probingAccounts[acc.id] ? 'Probing account quotas (Gemini 3.6 Flash & Claude Sonnet 4.6)...' : 'Kickstart rolling quota window with a minimal probe'">
                  <button
                    @click="handleKickstart($event, acc.id)"
                    :disabled="metricsStore.probingAccounts[acc.id]"
                    class="text-[9px] text-[var(--status-warn-color)] hover:opacity-80 px-0.5 cursor-pointer font-bold transition-all"
                    :class="{ 'animate-pulse opacity-70 cursor-wait': metricsStore.probingAccounts[acc.id] }"
                  >
                    {{ metricsStore.probingAccounts[acc.id] ? '⌛' : '⚡' }}
                  </button>
                </AppTooltip>
                <AppTooltip text="Rename label">
                  <button @click="startRename($event, acc)" class="text-[9px] text-[var(--dim-color)] hover:text-[var(--fg-color)] px-0.5">✎</button>
                </AppTooltip>
                <AppTooltip v-if="!acc.is_active && accountStore.googleAccounts.length > 1" text="Remove account">
                  <button @click="handleRemoveAccount($event, acc.id)" class="text-[9px] text-[var(--status-error-color)] hover:opacity-80 px-0.5">✕</button>
                </AppTooltip>
              </template>
            </div>
          </div>

          <div v-if="accountStore.googleAccounts.length === 0" class="px-2 py-2 text-[10px] text-[var(--dim-color)] text-center">
            No registered Google accounts
          </div>
        </div>

        <div class="border-t border-[var(--border-color)] pt-1 mt-0.5">
          <AppTooltip text="Open terminal to authenticate a new Google account">
            <button
              @click="handleAddAccount"
              class="w-full flex items-center justify-center gap-1.5 px-2 py-1 text-[10px] font-bold text-[var(--fg-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer border border-dashed border-[var(--border-color)]"
            >
              <span>+ Add Google Account</span>
            </button>
          </AppTooltip>
        </div>
      </div>
    </div>

    <!-- DeepSeek Wallet & API Key Popover -->
    <div class="relative deepseek-wallet-container">
      <AppTooltip text="DeepSeek API key & wallet balance" position="top">
        <button
          @click="toggleDeepSeekDropdown"
          class="h-6 flex items-center gap-1.5 px-2 border border-[var(--border-color)] bg-[var(--bg-color)] hover:border-[var(--accent-color)] text-[10px] font-mono tracking-tight transition-colors cursor-pointer shrink-0 select-none"
        >
          <DeepSeekIcon :size="14" class="shrink-0" />
          <span class="font-bold tracking-tight leading-none" :class="deepSeekBalanceClass">
            {{ deepSeekBalanceDisplay }}
          </span>
          <span class="text-[8px] text-[var(--dim-color)] shrink-0 ml-0.5 leading-none">▲</span>
        </button>
      </AppTooltip>

      <!-- DeepSeek Upward Dropdown Popover -->
      <div
        v-if="isDeepSeekDropdownOpen"
        class="absolute right-0 bottom-full mb-1.5 w-72 sm:w-80 max-w-[calc(100vw-4rem)] bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl z-50 p-2 flex flex-col gap-2 font-mono text-[11px]"
      >
        <div class="px-1 py-0.5 text-[9px] text-[var(--dim-color)] uppercase tracking-wider border-b border-[var(--border-color)] flex justify-between items-center">
          <span>DeepSeek Wallet</span>
          <button
            @click="refreshDeepSeekBalance"
            class="hover:text-[var(--accent-color)] cursor-pointer text-[10px]"
            :class="{ 'animate-spin': isRefreshingDeepSeek }"
          >
            ↻
          </button>
        </div>

        <!-- Balance Details Card -->
        <div class="p-2 border border-[var(--border-color)] bg-[var(--ui-bg-card)] flex flex-col gap-1 text-[10px]">
          <div class="flex justify-between items-center">
            <span class="text-[var(--dim-color)]">Total Balance:</span>
            <span class="font-bold text-[var(--status-ok-color)]">{{ deepSeekTotalBalance }}</span>
          </div>
          <div class="flex justify-between items-center text-[9px]">
            <span class="text-[var(--dim-color)]">Topped-Up (Cash):</span>
            <span>{{ deepSeekToppedUpBalance }}</span>
          </div>
          <div class="flex justify-between items-center text-[9px]">
            <span class="text-[var(--dim-color)]">Granted (Bonus):</span>
            <span>{{ deepSeekGrantedBalance }}</span>
          </div>
        </div>

        <!-- Active Key / Accounts Section -->
        <div v-if="accountStore.deepseekAccounts.length > 0" class="flex flex-col gap-1 border-t border-[var(--border-color)] pt-1.5">
          <span class="text-[9px] text-[var(--dim-color)] uppercase">Configured Keys</span>
          <div
            v-for="acc in accountStore.deepseekAccounts"
            :key="acc.id"
            @click="handleSwitch(acc.id)"
            class="flex items-center justify-between px-1.5 py-1 hover:bg-[var(--ui-bg-card)] cursor-pointer text-[10px]"
            :class="{ 'font-bold text-[var(--accent-color)] bg-[var(--ui-bg-card)]': acc.is_active }"
          >
            <div class="flex items-center gap-1.5 truncate min-w-0">
              <span class="text-[9px]" :class="acc.is_active ? 'text-[var(--status-ok-color)]' : 'text-[var(--dim-color)]'">
                {{ acc.is_active ? '●' : '○' }}
              </span>
              <span class="truncate">{{ acc.label || 'DeepSeek Key' }}</span>
            </div>
            <div class="flex items-center gap-1">
              <button
                v-if="!acc.is_active && accountStore.deepseekAccounts.length > 1"
                @click.stop="handleRemoveAccount($event, acc.id)"
                class="text-[9px] text-[var(--status-error-color)] hover:opacity-80 px-0.5"
              >
                ✕
              </button>
            </div>
          </div>
        </div>

        <!-- API Key Input Section -->
        <div class="border-t border-[var(--border-color)] pt-1.5 flex flex-col gap-1.5">
          <div class="flex justify-between items-center text-[9px] text-[var(--dim-color)] uppercase">
            <span>{{ showKeyInput ? 'Set API Key' : 'API Key' }}</span>
            <button
              @click="showKeyInput = !showKeyInput"
              class="text-[9px] text-[var(--accent-color)] hover:underline cursor-pointer"
            >
              {{ showKeyInput ? 'Cancel' : 'Update Key' }}
            </button>
          </div>

          <div v-if="showKeyInput" class="flex flex-col gap-1.5">
            <AppInput
              v-model="newDeepSeekKey"
              type="password"
              placeholder="sk-..."
              class="text-[10px] w-full"
            />
            <AppInput
              v-model="newDeepSeekLabel"
              placeholder="Label (e.g. Personal, Work)"
              class="text-[10px] w-full"
            />
            <button
              @click="handleSaveDeepSeekKey"
              :disabled="!newDeepSeekKey.trim()"
              class="px-2 py-1 text-[10px] font-bold border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 disabled:opacity-50 cursor-pointer"
            >
              Save Key
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
