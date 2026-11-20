import { defineStore } from 'pinia';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import { daemonCall } from '../utils/daemonClient';
import { useCoreStore } from './coreStore';
import type { AccountRecord, DeepSeekBalanceResponse } from '../types';
import type { AgySession } from '../types/generated/AgySession';

export const useAccountStore = defineStore('account', {
  state: () => ({
    accounts: [] as AccountRecord[],
    activeAccount: null as AccountRecord | null,
    deepseekBalance: null as DeepSeekBalanceResponse | null,
    isFetchingDeepSeekBalance: false,
    isSwitchingAccount: false,
    isAddingAccount: false,
    pendingSwitchTargetId: null as string | null,
    activeSessionsWarning: [] as AgySession[],
    showSwitchWarningModal: false,
  }),
  getters: {
    googleAccounts(state): AccountRecord[] {
      return state.accounts.filter(a => (a.provider || 'google') === 'google');
    },
    activeGoogleAccount(state): AccountRecord | null {
      return state.accounts.find(a => a.is_active && (a.provider || 'google') === 'google') || null;
    },
    deepseekAccounts(state): AccountRecord[] {
      return state.accounts.filter(a => a.provider === 'deepseek');
    },
    activeDeepseekAccount(state): AccountRecord | null {
      return state.accounts.find(a => a.is_active && a.provider === 'deepseek') || null;
    },
  },
  actions: {
    setAccounts(accounts: AccountRecord[]) {
      this.accounts = accounts;
      this.activeAccount =
        accounts.find(a => a.is_active && (a.provider || 'google') === 'google') ||
        accounts.find(a => a.is_active) ||
        null;
    },
    async fetchAccounts(): Promise<AccountRecord[]> {
      try {
        const accounts = await daemonCall('list_accounts', {});
        this.setAccounts(accounts);
        this.fetchDeepSeekBalance().catch(reportAsyncFailure);
        return this.accounts;
      } catch (err) {
        console.error('Failed to fetch accounts:', err);
        return [];
      }
    },
    async requestAccountSwitch(accountId: string): Promise<boolean> {
      if (this.activeAccount?.id === accountId) return true;
      try {
        const activeSessions = await daemonCall('check_active_agent_sessions', {});
        if (activeSessions.length > 0) {
          this.pendingSwitchTargetId = accountId;
          this.activeSessionsWarning = activeSessions;
          this.showSwitchWarningModal = true;
          return false;
        }
        await this.executeAccountSwitch(accountId, false);
        return true;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        useCoreStore().addStatusLog(`Account switch check failed: ${msg}`);
        return false;
      }
    },
    async confirmAccountSwitch(relaunchActive: boolean = false) {
      if (this.pendingSwitchTargetId) {
        const targetId = this.pendingSwitchTargetId;
        this.showSwitchWarningModal = false;
        this.pendingSwitchTargetId = null;
        await this.executeAccountSwitch(targetId, relaunchActive);
      }
    },
    async executeAccountSwitch(accountId: string, relaunchActive: boolean = false): Promise<boolean> {
      this.isSwitchingAccount = true;
      const coreStore = useCoreStore();
      try {
        await daemonCall('switch_account', { accountId, relaunchActive });
        await this.fetchAccounts();
        coreStore.addStatusLog(
          relaunchActive
            ? 'Account switched & active sessions restarted'
            : 'Account switched (active sessions preserved)'
        );
        return true;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Account switch failed: ${msg}`);
        return false;
      } finally {
        this.isSwitchingAccount = false;
      }
    },
    async initiateAddAccount(): Promise<boolean> {
      this.isAddingAccount = true;
      const coreStore = useCoreStore();
      try {
        await daemonCall('initiate_add_account', {});
        coreStore.addStatusLog('Authentication terminal spawned. Complete Google login.');
        return true;
      } catch (err: unknown) {
        this.isAddingAccount = false;
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Failed to start add account flow: ${msg}`);
        return false;
      }
    },
    async removeAccount(accountId: string): Promise<boolean> {
      const coreStore = useCoreStore();
      try {
        await daemonCall('remove_account', { accountId });
        await this.fetchAccounts();
        coreStore.addStatusLog('Account removed successfully');
        return true;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Failed to remove account: ${msg}`);
        return false;
      }
    },
    async renameAccount(accountId: string, label: string): Promise<boolean> {
      const coreStore = useCoreStore();
      try {
        await daemonCall('rename_account', { accountId, label });
        await this.fetchAccounts();
        coreStore.addStatusLog('Account renamed successfully');
        return true;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Failed to rename account: ${msg}`);
        return false;
      }
    },
    async fetchDeepSeekBalance(): Promise<DeepSeekBalanceResponse | null> {
      this.isFetchingDeepSeekBalance = true;
      try {
        const res = await daemonCall('get_deepseek_balance', {});
        this.deepseekBalance = res;
        return res;
      } catch (err) {
        console.error('Failed to fetch DeepSeek balance:', err);
        return null;
      } finally {
        this.isFetchingDeepSeekBalance = false;
      }
    },
    async saveDeepSeekApiKey(apiKey: string, label?: string): Promise<string | null> {
      const coreStore = useCoreStore();
      try {
        const id = await daemonCall('save_deepseek_api_key', { apiKey, email: null, label: label || null });
        await this.fetchAccounts();
        await this.fetchDeepSeekBalance();
        coreStore.addStatusLog('DeepSeek API key saved successfully');
        return id;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Failed to save DeepSeek key: ${msg}`);
        return null;
      }
    },
  },
});
