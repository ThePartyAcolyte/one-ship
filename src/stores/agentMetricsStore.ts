import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import { useDialogueStore } from './dialogueStore';
import type { AccountRecord } from '../types';
import type { AccountQuotaRecord } from '../types/generated/AccountQuotaRecord';
import type { AgentContextStats } from '../types/generated/AgentContextStats';
import type { AgentSessionMetrics } from '../types/generated/AgentSessionMetrics';
import type { AgentSessionMetricsRecord } from '../types/generated/AgentSessionMetricsRecord';
import type { AgentUsageUpdatePayload } from '../types/generated/AgentUsageUpdatePayload';
import type { CacheUsageStats } from '../types/generated/CacheUsageStats';
import type { KeyringWarmupStatusPayload } from '../types/generated/KeyringWarmupStatusPayload';
import type { UsageWindowStats } from '../types/generated/UsageWindowStats';

export type {
  UsageWindowStats,
  CacheUsageStats,
  AgentContextStats,
  AgentSessionMetrics,
  AgentSessionMetricsRecord,
  AccountQuotaRecord,
};

export type QuotaGaugeStyle = 'concentric' | 'bars' | 'classic';

export const useAgentMetricsStore = defineStore('agentMetrics', {
  state: () => ({
    quotaGaugeStyle: ((typeof localStorage !== 'undefined'
      ? localStorage.getItem('oneship_quota_gauge_style')
      : null) as QuotaGaugeStyle | null) || 'concentric',
    cachedMetrics: {} as Record<string, AgentSessionMetrics>,
    contextThresholdPct: 30.0,
    activeSessionMetrics: null as AgentSessionMetrics | null,
    accountQuotas: [] as AccountQuotaRecord[],
    probingAccounts: {} as Record<string, boolean>,
    isKeyringWarmupInProgress: false,
    warmupAccountEmail: '',
    warmupAccountId: '',
    warmupPool: '',
  }),

  actions: {
    setQuotaGaugeStyle(style: QuotaGaugeStyle) {
      this.quotaGaugeStyle = style;
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('oneship_quota_gauge_style', style);
      }
    },

    setCachedMetrics(agentId: string, metrics: AgentSessionMetrics) {
      this.cachedMetrics[agentId] = metrics;
    },

    async syncGlobalConfig() {
      try {
        const cfg = await daemonCall('get_global_config', {});
        const pluginsObj =
          cfg && typeof cfg === 'object' && 'plugins' in cfg && cfg.plugins && typeof cfg.plugins === 'object'
            ? (cfg.plugins as Record<string, unknown>)
            : null;
        const appearanceObj =
          pluginsObj && 'appearance' in pluginsObj && pluginsObj.appearance && typeof pluginsObj.appearance === 'object'
            ? (pluginsObj.appearance as Record<string, unknown>)
            : null;
        const style = appearanceObj?.quota_gauge_style;
        if (style === 'concentric' || style === 'bars' || style === 'classic') {
          this.setQuotaGaugeStyle(style);
        }
      } catch (err) {
        console.error('[agentMetricsStore] Failed to sync global config for quota gauge style:', err);
      }
    },

    async fetchContextThreshold(projectPath?: string) {
      try {
        const dialogueStore = useDialogueStore();
        const pPath =
          projectPath ||
          (dialogueStore.activeAgentId
            ? dialogueStore.resolveAgentProjectPath(dialogueStore.activeAgentId)
            : null);
        this.contextThresholdPct = await daemonCall('get_context_threshold', {
          projectPath: pPath ?? null,
        });
      } catch (err) {
        console.error('[agentMetricsStore] Failed to fetch context threshold:', err);
      }
    },

    /** Metrics of one agent; `conversationId` is a ONE-SHIP Conversation id. */
    async fetchActiveMetrics(projectPath: string, agentId: string, conversationId?: string) {
      try {
        const metrics = await daemonCall('get_agent_metrics', {
          projectPath,
          agentId,
          conversationId: conversationId ?? null,
        });
        if (metrics) {
          const prev = this.activeSessionMetrics;
          if (
            prev &&
            prev.source === 'agent-usage-update' &&
            metrics.tokens_used_count == null &&
            prev.tokens_used_count != null
          ) {
            metrics.tokens_used_count = prev.tokens_used_count;
            metrics.cost_dollars = metrics.cost_dollars ?? prev.cost_dollars;
            metrics.max_context_tokens_count = metrics.max_context_tokens_count ?? prev.max_context_tokens_count;
            if (!metrics.context && prev.context) {
              metrics.context = prev.context;
            }
          }
          this.activeSessionMetrics = metrics;
        }
      } catch (err) {
        console.warn('[agentMetricsStore] Failed to fetch active agent metrics:', err);
      }
    },

    async fetchSessionMetrics(agyConversationId: string) {
      try {
        const metrics = await daemonCall('get_session_metrics', {
          agyConversationId,
        });
        if (metrics && this.activeSessionMetrics) {
          this.activeSessionMetrics.context = {
            used_percent: metrics.context_used_pct,
            remaining_percent: Math.max(0, 100 - metrics.context_used_pct),
            cache: {
              fresh_input_tokens_count: metrics.fresh_tokens_count,
              read_tokens_count: metrics.cache_read_tokens_count,
              creation_tokens_count: metrics.cache_creation_tokens_count,
              hit_percent: metrics.cache_hit_pct,
              ttl_seconds: null,
            },
          };
        }
      } catch (err) {
        console.error('[agentMetricsStore] Failed to fetch session metrics:', err);
      }
    },

    async fetchAccountQuotas() {
      try {
        this.accountQuotas = await daemonCall('get_account_quotas', {});
      } catch (err) {
        console.error('[agentMetricsStore] Failed to fetch account quotas:', err);
      }
    },

    async kickstartAccountQuota(accountId: string) {
      try {
        this.probingAccounts[accountId] = true;
        await daemonCall('kickstart_account_quota', { accountId });
        await this.fetchAccountQuotas();
      } catch (err) {
        console.error('[agentMetricsStore] Failed to kickstart account quota:', err);
      } finally {
        this.probingAccounts[accountId] = false;
      }
    },

    async toggleAccountWarmup(accountId: string, enabled: boolean) {
      try {
        await daemonCall('toggle_account_warmup', { accountId, enabled });
        await this.fetchAccountQuotas();
      } catch (err) {
        console.error('[agentMetricsStore] Failed to toggle account warmup:', err);
      }
    },

    async applyKeyringWarmupStatus(payload: KeyringWarmupStatusPayload) {
      if (payload?.in_progress !== undefined) {
        this.isKeyringWarmupInProgress = !!payload.in_progress;
        this.warmupAccountEmail = payload.email || '';
        this.warmupAccountId = payload.account_id || '';
        this.warmupPool = payload.pool || 'gemini-pool';
        if (!payload.in_progress) {
          await this.fetchAccountQuotas();
        }
      }
    },

    applyUsageUpdate(payload: AgentUsageUpdatePayload) {
      const dialogueStore = useDialogueStore();
      if (!payload?.agent_id) return;
      if (dialogueStore.isCurrentActiveAgent(payload.agent_id, payload.project_path)) {
        if (!this.activeSessionMetrics) {
          this.activeSessionMetrics = {
            agent_key: null,
            conversation_id: null,
            provider:
              payload.runner === 'opencode'
                ? 'deepseek'
                : payload.runner === 'claude'
                  ? 'claude'
                  : 'antigravity',
            model: payload.model ?? null,
            status: 'active',
            five_hour_window: null,
            weekly_window: null,
            context: null,
            source: 'agent-usage-update',
            captured_at_epoch_seconds: Math.floor(Date.now() / 1000),
            tokens_used_count: null,
            cost_dollars: null,
            max_context_tokens_count: null,
          };
        }
        const metrics = this.activeSessionMetrics;
        metrics.tokens_used_count = payload.tokens_used_count ?? null;
        metrics.cost_dollars = payload.cost_dollars ?? null;
        metrics.max_context_tokens_count = payload.max_context_tokens_count ?? null;
        if (payload.model) metrics.model = payload.model;
        if (
          payload.tokens_used_count !== undefined &&
          payload.tokens_used_count !== null &&
          payload.max_context_tokens_count &&
          payload.max_context_tokens_count > 0
        ) {
          const usedPct = Math.min(100, Math.round((payload.tokens_used_count / payload.max_context_tokens_count) * 100));
          metrics.context = {
            used_percent: usedPct,
            remaining_percent: Math.max(0, 100 - usedPct),
            cache: null,
          };
        }
      }
    },

    getRecommendedAccountForModel(
      accounts: AccountRecord[],
      runner?: string,
      model?: string
    ): {
      recommendedAccountId: string;
      accountQuotasMap: Record<string, { pct: number; resetsIn?: string; viable: boolean }>;
    } {
      const runnerKey = (runner || '').toLowerCase();
      const isOpencode = runnerKey === 'opencode';
      const isClaude = runnerKey === 'claude';
      const mLower = (model || '').toLowerCase();
      const targetPool = isOpencode
        ? 'deepseek'
        : mLower.includes('claude') || mLower.includes('opus') || mLower.includes('sonnet') || mLower.includes('gpt')
          ? '3p-pool'
          : 'gemini-pool';

      const quotaMatches = (
        r: { account_id: string; provider?: string; model?: string },
        accountId: string
      ): boolean => {
        if (r.account_id !== accountId) return false;
        if (isOpencode) return r.provider === 'deepseek';
        if (isClaude) return r.provider === 'claude';
        return r.model === targetPool;
      };

      const accountQuotasMap: Record<string, { pct: number; resetsIn?: string; viable: boolean }> = {};
      const relevantAccounts = accounts.filter(a => {
        if (isOpencode) return a.provider === 'deepseek';
        if (isClaude) return a.provider === 'claude';
        return !a.provider || a.provider === 'antigravity' || a.provider === 'google';
      });

      for (const acc of relevantAccounts) {
        const q = this.accountQuotas.find(r => quotaMatches(r, acc.id));
        if (q) {
          const fiveH = q.five_hour_remaining_pct ?? 100;
          const weekly = q.weekly_remaining_pct ?? 100;
          const viable = fiveH > 5 && weekly > 0;
          let resetsIn: string | undefined;
          if (q.weekly_resets_at) {
            resetsIn = q.weekly_resets_at;
          } else if (q.weekly_reset_epoch_seconds) {
            const diffSec = q.weekly_reset_epoch_seconds - Math.floor(Date.now() / 1000);
            if (diffSec > 0) {
              const hours = Math.floor(diffSec / 3600);
              const days = Math.floor(hours / 24);
              resetsIn = days > 0 ? `${days}d ${hours % 24}h` : `${hours}h`;
            }
          }
          accountQuotasMap[acc.id] = { pct: Math.round(fiveH), resetsIn, viable };
        } else {
          accountQuotasMap[acc.id] = { pct: 100, viable: true };
        }
      }

      const sorted = [...relevantAccounts].sort((a, b) => {
        const qa = this.accountQuotas.find(r => quotaMatches(r, a.id));
        const qb = this.accountQuotas.find(r => quotaMatches(r, b.id));
        const aViable = accountQuotasMap[a.id]?.viable ?? true;
        const bViable = accountQuotasMap[b.id]?.viable ?? true;
        if (aViable !== bViable) return aViable ? -1 : 1;

        const aReset = qa?.weekly_reset_epoch_seconds || Number.MAX_SAFE_INTEGER;
        const bReset = qb?.weekly_reset_epoch_seconds || Number.MAX_SAFE_INTEGER;
        if (aReset !== bReset) return aReset - bReset;

        const aPct = qa?.five_hour_remaining_pct ?? 100;
        const bPct = qb?.five_hour_remaining_pct ?? 100;
        return bPct - aPct;
      });

      return {
        recommendedAccountId: sorted[0]?.id || '',
        accountQuotasMap,
      };
    },
  },
});
