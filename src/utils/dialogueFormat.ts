import type { AgentResponsePayload } from '../types/generated/AgentResponsePayload';
import type { ChatTurnRecord } from '../types/generated/ChatTurnRecord';
import type { DocumentCommentRecord } from '../types/generated/DocumentCommentRecord';
import type { AttachedReviewFile } from '../stores/projectExplorerStore';
import type { ApprovalRequestData, DialogueArtifact, DialogueMessage } from '../stores/dialogueStore';

/** Roles stored in `agent_chat_turns.role`; any other value is a corrupt row. */
function messageRole(role: string): DialogueMessage['role'] {
  if (role === 'user' || role === 'agent' || role === 'system') return role;
  throw new Error(`Chat turn has unknown role '${role}'`);
}

function resolveImplicitFilePath(content: string, explicitFilePath?: string | null): {
  filePath?: string;
  isEphemeral: boolean;
} {
  let resolvedFilePath = explicitFilePath ?? undefined;
  let isEphemeral = false;
  if (!resolvedFilePath && content && !content.includes('\n') && content.length < 1024) {
    const trimmed = content.trim().replace(/^\/+/, '');
    if (
      trimmed.startsWith('scratchpad/') ||
      trimmed.startsWith('docs/') ||
      (trimmed.includes('/') && /\.[a-zA-Z0-9_-]+$/.test(trimmed))
    ) {
      resolvedFilePath = trimmed;
      isEphemeral = false;
    }
  }
  return { filePath: resolvedFilePath, isEphemeral };
}

export function chatTurnToMessage(turn: ChatTurnRecord): DialogueMessage {
  let artifact: DialogueArtifact | undefined;
  if (turn.artifact_content || turn.artifact_file_path) {
    const resolvedContent = turn.artifact_content || '';
    const { filePath: autoPath } = resolveImplicitFilePath(resolvedContent, turn.artifact_file_path);
    const resolvedFilePath = turn.artifact_file_path ?? autoPath;
    const isEphemeral = autoPath && !turn.artifact_file_path ? false : !!turn.is_ephemeral;
    const fileLabel = resolvedFilePath ? resolvedFilePath.split('/').pop() : null;
    const firstLine =
      turn.artifact_label ||
      fileLabel ||
      (resolvedContent ? resolvedContent.split('\n')[0].replace(/^[#`\-\s]+/, '').slice(0, 40) : null) ||
      turn.artifact_type ||
      'artifact';

    artifact = {
      id: turn.id != null ? `art-${turn.id}` : `art-${crypto.randomUUID()}`,
      agentId: turn.agent_id,
      type: turn.artifact_type || 'markdown',
      lang: turn.artifact_lang ?? undefined,
      content: resolvedContent,
      timestamp: turn.timestamp,
      label: firstLine,
      filePath: resolvedFilePath,
      isEphemeral,
    };
  }

  let approval: ApprovalRequestData | undefined;
  if (turn.approval_payload) {
    try {
      const parsed = JSON.parse(turn.approval_payload);
      if (parsed && typeof parsed === 'object') {
        approval = {
          title: parsed.title || 'Proposal',
          description: parsed.description || '',
          options: Array.isArray(parsed.options) ? parsed.options : undefined,
          status: parsed.status || 'pending',
          resolvedAt: parsed.resolvedAt,
          feedback: parsed.feedback,
        };
      }
    } catch (e) {
      console.warn('[dialogueFormat] Failed to parse turn approval payload:', e);
    }
  }

  return {
    id: `turn-${turn.id || turn.timestamp}`,
    turnId: turn.id ?? undefined,
    role: messageRole(turn.role),
    agentId: turn.agent_id,
    dialogue: turn.dialogue,
    artifact,
    approval,
    timestamp: turn.timestamp,
    terminalSnapshot: turn.terminal_snapshot ?? undefined,
    conversationId: turn.conversation_id,
    topic: turn.topic ?? undefined,
  };
}

export function buildResponseArtifact(payload: AgentResponsePayload): DialogueArtifact | undefined {
  if (!payload.artifact) return undefined;
  const { filePath: autoPath } = resolveImplicitFilePath(payload.artifact, payload.file_path);
  const filePath = payload.file_path ?? autoPath;
  const isEphemeral = autoPath && !payload.file_path ? false : !!payload.is_ephemeral;
  const fileLabel = filePath ? filePath.split('/').pop() : null;
  const firstLine =
    fileLabel ||
    payload.artifact.split('\n')[0].replace(/^[#`\-\s]+/, '').slice(0, 40) ||
    payload.artifact_type ||
    'artifact';

  return {
    id: `art-${crypto.randomUUID()}`,
    agentId: payload.agent_id,
    type: payload.artifact_type || 'markdown',
    lang: payload.artifact_lang ?? undefined,
    content: payload.artifact,
    timestamp: payload.timestamp,
    label: firstLine,
    filePath,
    isEphemeral,
  };
}

export function formatAttachedReviewMessage(
  rawText: string,
  attachedFiles: AttachedReviewFile[],
  documentComments: DocumentCommentRecord[]
): string {
  if (attachedFiles.length === 0) return rawText;
  let block = '';
  const hasInlineComments = attachedFiles.some(
    f => documentComments.some(c => c.file_path === f.rel_path && c.status === 'open')
  );

  if (hasInlineComments) {
    block += `[REVIEW CONTEXT: ${attachedFiles.length} FILE(S) ATTACHED]\n`;
    for (let i = 0; i < attachedFiles.length; i++) {
      const file = attachedFiles[i];
      const ext = file.name.split('.').pop()?.toUpperCase() || 'FILE';
      const placeholder = `@${ext}_${i + 1}`;
      block += `\n### ${placeholder} File: ${file.rel_path}\n`;
      const fileComments = documentComments.filter(
        c => c.file_path === file.rel_path && c.status === 'open'
      );
      if (fileComments.length > 0) {
        for (const c of fileComments) {
          block += `- [Comment #${c.id}] Lines L${c.start_line}-L${c.end_line}: "${c.selected_text}"\n  Feedback: "${c.comment_text}"\n`;
        }
      } else {
        block += `(Referenced file)\n`;
      }
    }
  } else {
    block += `[ATTACHED CONTEXT & REFERENCES]:\n`;
    for (let i = 0; i < attachedFiles.length; i++) {
      const file = attachedFiles[i];
      const ext = file.name.split('.').pop()?.toUpperCase() || 'FILE';
      const placeholder = `@${ext}_${i + 1}`;
      const typeLabel =
        file.doc_type === 'image' ? 'Image preview' : file.is_external ? 'External file' : 'Workspace file';
      block += `- ${placeholder} ${file.rel_path} (${typeLabel})\n`;
    }
  }

  return rawText ? `${rawText}\n\n---\n${block.trim()}` : block.trim();
}
