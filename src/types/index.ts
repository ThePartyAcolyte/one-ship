export interface SkillStatus {
  is_installed: boolean;
  installed_version?: string | null;
  current_version: string;
  is_outdated: boolean;
  is_symlinked?: boolean;
  schema_version?: string | null;
  needs_migration?: boolean;
}

export type { GitStatus } from './generated/GitStatus';
export type { GitRemote } from './generated/GitRemote';
export type { ForgeProviderKind } from './generated/ForgeProviderKind';

export type { ScriptOption } from './generated/ScriptOption';
export type { ScriptEntry } from './generated/ScriptEntry';

/** Launch option values keyed by `ScriptOption.key`; omitted keys take their declared default. */
export type ScriptOptionValues = Record<string, boolean | string>;

// Types of the daemon API, generated from Rust (`src/types/generated/`).
export type { ServiceStartOutcome } from './generated/ServiceStartOutcome';
export type { AgentState } from './generated/AgentState';
export type { ProjectAgentState } from './generated/ProjectAgentState';
export type { AccountSummary as AccountRecord } from './generated/AccountSummary';
export type { DeepSeekBalanceInfo } from './generated/DeepSeekBalanceInfo';
export type { DeepSeekBalanceResponse } from './generated/DeepSeekBalanceResponse';
export type { TaskExecutionRecord } from './generated/TaskExecutionRecord';
export type { AgentSessionMetrics } from './generated/AgentSessionMetrics';
export type { DiagnosticState } from './generated/DiagnosticState';
export type { HostCapabilities } from './generated/HostCapabilities';
export type { IntegrationCapability } from './generated/IntegrationCapability';

export type { AgentPersonaInput } from './generated/AgentPersonaInput';
export type { RunnerSchema } from './generated/RunnerSchema';
export type { RunnerModel } from './generated/RunnerModel';

export type { HydratedProject as ProjectRecord } from './generated/HydratedProject';
export type { ProjectInitFlagRecord } from './generated/ProjectInitFlagRecord';
export type { ProjectSkillRecord } from './generated/ProjectSkillRecord';
export type { MarketplaceSkillItem } from './generated/MarketplaceSkillItem';
export type { ProfilerCatalog } from './generated/ProfilerCatalog';
export type { ProfilerManifestSummary } from './generated/ProfilerManifestSummary';
export type { ProfilerManifestFault } from './generated/ProfilerManifestFault';
export type { ProfilerViewerType } from './generated/ProfilerViewerType';
export type { TargetRunRecord } from './generated/TargetRunRecord';
export type { RunProfile } from './generated/RunProfile';
export type { RunProcessProfile } from './generated/RunProcessProfile';
export type { RunComparison } from './generated/RunComparison';
export type { SystemLogRecord } from './generated/SystemLogRecord';
export type { Snapshot } from './generated/Snapshot';
export type { DocumentItem } from './generated/DocumentItem';
export type { DocumentContent } from './generated/DocumentContent';
export type { DocumentCommentRecord } from './generated/DocumentCommentRecord';
export type { AttachedFileResult } from './generated/AttachedFileResult';
export type { GitCommitInfo } from './generated/GitCommitInfo';
export type { CommitFileChange } from './generated/CommitFileChange';
export type { WorkingTreeFile } from './generated/WorkingTreeFile';
export type { VcsAction } from './generated/VcsAction';
export type { FileTree } from './generated/FileTree';
export type { FileTreeNode } from './generated/FileTreeNode';
export type { PluginConfigSchema } from './generated/PluginConfigSchema';
export type { ConfigFieldSchema } from './generated/ConfigFieldSchema';
export type { ThemeColors } from './generated/ThemeColors';
export type { DictationStatus } from './generated/DictationStatus';
export type { DictationMode } from './generated/DictationMode';
export type { DictationState } from './generated/DictationState';
export type { GeminiApiKeyStatus } from './generated/GeminiApiKeyStatus';
export type { SystemMetrics } from './generated/SystemMetrics';
export type { ServiceMetrics } from './generated/ServiceMetrics';
export type { ProjectElectronicsStatus } from './generated/ProjectElectronicsStatus';
export type { DiscoveredMcuPort } from './generated/DiscoveredMcuPort';
export type { DiscoveredInstrument } from './generated/DiscoveredInstrument';
export type { FirmwareOpResult } from './generated/FirmwareOpResult';
export type { ElectronicsToolStatus } from './generated/ElectronicsToolStatus';
export type { ElectronicsError } from './generated/ElectronicsError';
export type { OscilloscopeCaptureResult } from './generated/OscilloscopeCaptureResult';
export type { OscilloscopeMetrics } from './generated/OscilloscopeMetrics';
export type { OscilloscopeChannelConfig } from './generated/OscilloscopeChannelConfig';
export type { OscilloscopeTriggerConfig } from './generated/OscilloscopeTriggerConfig';
export type { OscilloscopeCalibrationResult } from './generated/OscilloscopeCalibrationResult';
export type { OscilloscopeDescriptor } from './generated/OscilloscopeDescriptor';
export type { CalibrateOscilloscopeArgs } from './generated/CalibrateOscilloscopeArgs';
export type { LogicCaptureResult } from './generated/LogicCaptureResult';
export type { DecodedProtocolPacket } from './generated/DecodedProtocolPacket';
export type { PlatformioBoard } from './generated/PlatformioBoard';
export type { ListPlatformioBoardsArgs } from './generated/ListPlatformioBoardsArgs';
export type { ScaffoldFirmwareProjectArgs } from './generated/ScaffoldFirmwareProjectArgs';
export type { ScaffoldFirmwareResult } from './generated/ScaffoldFirmwareResult';

export interface PendingApproval {
  task_id: string;
  phase: string;
  payload_markdown: string;
  project_path?: string;
  approval_type?: string;
  agent_id?: string;
  options?: string[];
}

