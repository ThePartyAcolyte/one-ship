import { ref, computed, onUnmounted, type ComputedRef } from 'vue';
import { useProjectExplorerStore } from '../stores/projectExplorerStore';
import { useDictationStore } from '../stores/dictationStore';
import { createValueTarget } from '../utils/dictationTargets';
import type { ProjectRecord } from '../types';

export interface DisplayDocumentRef {
  relPath?: string;
  content?: string;
  title?: string;
  category?: string;
}

export function stripMarkdownFormatting(str: string): string {
  return str
    .replace(/^[\s#*>\-+]+/, '')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/_([^_]+)_/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/\[\s*[xX ]\s*\]/g, '')
    .replace(/<\/?[^>]+(>|$)/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
}

export function findLineRangeForSelection(
  rawContent: string,
  selectedText: string,
): { startLine: number; endLine: number } {
  const exactIndex = rawContent.indexOf(selectedText);
  if (exactIndex !== -1) {
    const beforeText = rawContent.slice(0, exactIndex);
    const startLine = beforeText.split('\n').length;
    const linesCount = selectedText.split('\n').length;
    return {
      startLine,
      endLine: startLine + linesCount - 1,
    };
  }

  const rawLines = rawContent.split('\n');
  const selSnippetLines = selectedText
    .split('\n')
    .map(l => l.trim())
    .filter(l => l.length > 0);

  if (selSnippetLines.length === 0) {
    return { startLine: 1, endLine: 1 };
  }

  const firstSnippet = stripMarkdownFormatting(selSnippetLines[0]);
  const lastSnippet = stripMarkdownFormatting(selSnippetLines[selSnippetLines.length - 1]);

  let matchedStart = -1;
  let matchedEnd = -1;

  for (let i = 0; i < rawLines.length; i++) {
    const cleanLine = stripMarkdownFormatting(rawLines[i]);
    if (!cleanLine) continue;

    if (cleanLine.includes(firstSnippet) || (firstSnippet.length > 5 && firstSnippet.includes(cleanLine))) {
      matchedStart = i + 1;

      if (selSnippetLines.length === 1) {
        matchedEnd = matchedStart;
        break;
      }

      for (let j = i; j < rawLines.length; j++) {
        const endCleanLine = stripMarkdownFormatting(rawLines[j]);
        if (endCleanLine.includes(lastSnippet) || (lastSnippet.length > 5 && lastSnippet.includes(endCleanLine))) {
          matchedEnd = j + 1;
          break;
        }
      }

      if (matchedEnd === -1) {
        matchedEnd = matchedStart + selSnippetLines.length - 1;
      }
      break;
    }
  }

  if (matchedStart === -1) {
    const words = firstSnippet.split(' ').filter(w => w.length >= 4);
    if (words.length > 0) {
      for (let i = 0; i < rawLines.length; i++) {
        const cleanLine = stripMarkdownFormatting(rawLines[i]);
        const matchCount = words.filter(w => cleanLine.includes(w)).length;
        if (matchCount >= Math.min(2, words.length)) {
          matchedStart = i + 1;
          matchedEnd = matchedStart + selSnippetLines.length - 1;
          break;
        }
      }
    }
  }

  if (matchedStart !== -1) {
    return {
      startLine: matchedStart,
      endLine: Math.max(matchedStart, matchedEnd !== -1 ? matchedEnd : matchedStart),
    };
  }

  return { startLine: 1, endLine: Math.max(1, selSnippetLines.length) };
}

export function useDocumentComments(
  project: ComputedRef<ProjectRecord | null>,
  displayContent: ComputedRef<DisplayDocumentRef | null>,
) {
  const explorerStore = useProjectExplorerStore();
  const dictationStore = useDictationStore();

  const showCommentPopover = ref(false);
  const popoverPosition = ref({ x: 0, y: 0 });
  const selectedSnippet = ref('');
  const selectedStartLine = ref(1);
  const selectedEndLine = ref(1);
  const newCommentText = ref('');
  const isRecordingVoice = computed(() => dictationStore.isActiveFor('comment'));
  const isSavingComment = ref(false);
  const showCommentsDrawer = ref(false);
  const contentContainerRef = ref<HTMLElement | null>(null);

  function handleTextSelection(_event?: MouseEvent) {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !displayContent.value?.content) {
      return;
    }

    const text = selection.toString().trim();
    if (!text || text.length < 2) return;

    const content = displayContent.value.content;
    const { startLine, endLine } = findLineRangeForSelection(content, text);

    selectedSnippet.value = text;
    selectedStartLine.value = startLine;
    selectedEndLine.value = endLine;
    newCommentText.value = '';

    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    popoverPosition.value = {
      x: Math.max(10, Math.min(window.innerWidth - 320, rect.left)),
      y: Math.max(50, rect.bottom + 8),
    };

    showCommentPopover.value = true;
  }

  function openCommentAt(params: {
    snippet: string;
    startLine: number;
    endLine: number;
    x: number;
    y: number;
  }) {
    selectedSnippet.value = params.snippet;
    selectedStartLine.value = Math.max(1, params.startLine);
    selectedEndLine.value = Math.max(selectedStartLine.value, params.endLine);
    newCommentText.value = '';
    popoverPosition.value = {
      x: Math.max(10, Math.min(window.innerWidth - 330, params.x)),
      y: Math.max(50, Math.min(window.innerHeight - 220, params.y)),
    };
    showCommentPopover.value = true;
  }

  function cancelComment() {
    showCommentPopover.value = false;
    selectedSnippet.value = '';
    newCommentText.value = '';
    if (isRecordingVoice.value) {
      dictationStore.cancel().catch((err: unknown) => {
        console.warn('[useDocumentComments] dictation cancel failed:', err);
      });
    }
  }

  async function saveComment() {
    if (isRecordingVoice.value) return;
    if (!displayContent.value?.relPath || !newCommentText.value.trim() || !project.value?.id) return;
    isSavingComment.value = true;
    try {
      await explorerStore.createDocumentComment({
        projectId: project.value.id,
        filePath: displayContent.value.relPath,
        startLine: selectedStartLine.value,
        endLine: selectedEndLine.value,
        selectedText: selectedSnippet.value,
        commentText: newCommentText.value.trim(),
        author: 'user',
      });

      const attached = explorerStore.attachedReviewFiles.find(
        f => f.rel_path === displayContent.value?.relPath,
      );
      if (attached) {
        attached.comment_count = explorerStore.activeDocumentOpenCommentCount;
      }

      cancelComment();
      showCommentsDrawer.value = true;
    } catch (err: unknown) {
      console.error('Failed to save review comment:', err);
    } finally {
      isSavingComment.value = false;
    }
  }

  async function toggleVoiceRecording() {
    if (isRecordingVoice.value) {
      await dictationStore.stop();
      return;
    }

    if (!project.value?.id) return;
    await dictationStore.start(createValueTarget(
      'comment',
      { kind: 'comment' },
      () => newCommentText.value,
      (value) => { newCommentText.value = value; },
    ));
  }

  function scrollToCommentLine(cmt: { start_line: number }) {
    if (!contentContainerRef.value) return;
    const container = contentContainerRef.value;
    const totalHeight = container.scrollHeight;
    const lineCount = displayContent.value?.content
      ? displayContent.value.content.split('\n').length
      : 100;
    const lineRatio = Math.min(1, Math.max(0, (cmt.start_line - 1) / Math.max(1, lineCount)));
    container.scrollTo({
      top: lineRatio * totalHeight,
      behavior: 'smooth',
    });
  }

  onUnmounted(() => {
    if (isRecordingVoice.value) {
      dictationStore.cancel().catch((err: unknown) => {
        console.warn('[useDocumentComments] dictation cancel on unmount failed:', err);
      });
    }
  });

  return {
    showCommentPopover,
    popoverPosition,
    selectedSnippet,
    selectedStartLine,
    selectedEndLine,
    newCommentText,
    isRecordingVoice,
    isSavingComment,
    showCommentsDrawer,
    contentContainerRef,
    handleTextSelection,
    openCommentAt,
    cancelComment,
    saveComment,
    toggleVoiceRecording,
    scrollToCommentLine,
  };
}
