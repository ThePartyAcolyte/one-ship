import { defineStore } from 'pinia';
import { open } from '@tauri-apps/plugin-dialog';
import { daemonCall } from '../utils/daemonClient';
import { useDialogueStore } from './dialogueStore';
import type { DocumentItem as GeneratedDocumentItem } from '../types/generated/DocumentItem';
import type { DocumentContent as GeneratedDocumentContent } from '../types/generated/DocumentContent';
import type { DocumentCommentRecord } from '../types/generated/DocumentCommentRecord';
import type { DocumentCommentResolvedPayload } from '../types/generated/DocumentCommentResolvedPayload';

export type DocumentItem = GeneratedDocumentItem;
export type DocumentContent = GeneratedDocumentContent;
export type DocumentComment = DocumentCommentRecord;

export interface AttachedReviewFile {
  id?: string;
  rel_path: string;
  name: string;
  category?: string;
  comment_count?: number;
  doc_type?: string;
  is_external?: boolean;
  preview_url?: string;
  size_bytes?: number;
}

export const useProjectExplorerStore = defineStore('projectExplorer', {
  state: () => ({
    projectDocuments: [] as DocumentItem[],
    openDocuments: [] as DocumentContent[],
    selectedDocument: null as DocumentContent | null,
    isLoadingDocument: false,
    documentComments: [] as DocumentComment[],
    attachedReviewFiles: [] as AttachedReviewFile[],
    explorerScope: 'docs' as 'docs' | 'codebase',
    explorerPath: '' as string,
    explorerClipboard: null as { items: string[]; operation: 'copy' | 'cut' } | null,
    unsavedDrafts: {} as Record<string, string>,
  }),

  getters: {
    activeDocumentComments(state): DocumentComment[] {
      if (!state.selectedDocument) return [];
      return state.documentComments.filter(
        c => c.file_path === state.selectedDocument?.rel_path
      );
    },

    activeDocumentOpenCommentCount(state): number {
      if (!state.selectedDocument) return 0;
      return state.documentComments.filter(
        c => c.file_path === state.selectedDocument?.rel_path && c.status === 'open'
      ).length;
    },
  },

  actions: {
    setExplorerPath(path: string) {
      this.explorerPath = path;
    },

    clearExplorerState() {
      this.projectDocuments = [];
      this.openDocuments = [];
      this.selectedDocument = null;
      this.documentComments = [];
      this.attachedReviewFiles = [];
      this.unsavedDrafts = {};
      this.explorerClipboard = null;
      this.explorerPath = '';
    },

    upsertOpenDocument(doc: DocumentContent) {
      const idx = this.openDocuments.findIndex(d => d.rel_path === doc.rel_path);
      if (idx !== -1) {
        this.openDocuments.splice(idx, 1, doc);
      } else {
        this.openDocuments.push(doc);
      }
    },

    async activateOpenTab(projectPath: string, relPath: string, projectId?: number) {
      const existing = this.openDocuments.find(d => d.rel_path === relPath);
      if (existing) {
        this.selectedDocument = existing;
        const dialogueStore = useDialogueStore();
        dialogueStore.activeArtifactId = null;
        if (projectId) {
          await this.loadDocumentComments(projectId, relPath);
        }
        return;
      }
      await this.selectProjectDocument(projectPath, relPath, projectId);
    },

    closeDocumentTab(relPath: string) {
      const idx = this.openDocuments.findIndex(d => d.rel_path === relPath);
      if (idx === -1) return;
      this.openDocuments.splice(idx, 1);
      delete this.unsavedDrafts[relPath];

      if (this.selectedDocument?.rel_path === relPath) {
        const nextDoc = this.openDocuments[idx] ?? this.openDocuments[idx - 1] ?? null;
        this.selectedDocument = nextDoc;
      }
    },

    async fetchProjectDocuments(projectPath: string, _scope?: string) {
      if (!projectPath) return;
      try {
        const sc = 'codebase';
        this.explorerScope = 'codebase';
        const docs = await daemonCall('get_project_documents', { projectPath, scope: sc });
        this.projectDocuments = docs;
      } catch (err) {
        console.error('[projectExplorerStore] Failed to fetch project documents:', err);
      }
    },

    async selectProjectDocument(projectPath: string, relPath: string, projectId?: number) {
      if (!projectPath || !relPath) return;
      this.isLoadingDocument = true;
      try {
        const doc = await daemonCall('read_project_document', { projectPath, relPath });
        this.upsertOpenDocument(doc);
        this.selectedDocument = doc;
        const dialogueStore = useDialogueStore();
        dialogueStore.activeArtifactId = null;

        if (projectId) {
          await this.loadDocumentComments(projectId, relPath);
        }
      } catch (err) {
        console.error('[projectExplorerStore] Failed to read project document:', err);
      } finally {
        this.isLoadingDocument = false;
      }
    },

    async saveProjectDocument(projectPath: string, relPath: string, content: string) {
      if (!projectPath || !relPath) return;
      try {
        const savedDoc = await daemonCall('save_project_document', {
          projectPath,
          relPath,
          content,
        });
        this.upsertOpenDocument(savedDoc);
        if (this.selectedDocument && this.selectedDocument.rel_path === relPath) {
          this.selectedDocument = savedDoc;
        }
        const dialogueStore = useDialogueStore();
        if (dialogueStore.activeArtifact && dialogueStore.activeArtifact.filePath === relPath) {
          dialogueStore.activeArtifact.content = content;
          dialogueStore.activeArtifact.timestamp = savedDoc.mtime_seconds;
        }
        delete this.unsavedDrafts[relPath];
        await this.fetchProjectDocuments(projectPath);
        return savedDoc;
      } catch (err) {
        console.error('[projectExplorerStore] Failed to save project document:', err);
        throw err;
      }
    },

    setUnsavedDraft(relPath: string, content: string) {
      if (!relPath) return;
      this.unsavedDrafts[relPath] = content;
    },

    clearUnsavedDraft(relPath: string) {
      if (!relPath) return;
      delete this.unsavedDrafts[relPath];
    },

    async loadDocumentComments(projectId: number, filePath?: string) {
      try {
        const comments = await daemonCall('list_document_comments', {
          projectId,
          filePath: filePath || null,
          status: null,
        });
        this.documentComments = comments;
      } catch (err) {
        console.error('[projectExplorerStore] Failed to load document comments:', err);
      }
    },

    setDocumentOpenCommentCount(filePath: string, openCount: number) {
      const updateTree = (items: DocumentItem[]): boolean => {
        for (const item of items) {
          if (!item.is_dir && item.rel_path === filePath) {
            item.open_comment_count = Math.max(0, openCount);
            return true;
          }
          if (item.children && updateTree(item.children)) {
            return true;
          }
        }
        return false;
      };
      updateTree(this.projectDocuments);
    },

    async createDocumentComment(payload: {
      projectId: number;
      filePath: string;
      startLine: number;
      endLine: number;
      selectedText: string;
      commentText: string;
      author?: string;
    }) {
      try {
        const newCmt = await daemonCall('create_document_comment', {
          projectId: payload.projectId,
          filePath: payload.filePath,
          startLine: payload.startLine,
          endLine: payload.endLine,
          selectedText: payload.selectedText,
          commentText: payload.commentText,
          author: payload.author || 'user',
        });
        this.documentComments.push(newCmt);
        const openCount = this.documentComments.filter(
          c => c.file_path === payload.filePath && c.status === 'open'
        ).length;
        this.setDocumentOpenCommentCount(payload.filePath, openCount);
        return newCmt;
      } catch (err) {
        console.error('[projectExplorerStore] Failed to create document comment:', err);
        throw err;
      }
    },

    async resolveDocumentComment(commentId: string, resolvedBy?: string) {
      try {
        await daemonCall('resolve_document_comment', {
          commentId,
          resolvedBy: resolvedBy || 'user',
        });
        const cmt = this.documentComments.find(c => c.id === commentId);
        if (cmt) {
          cmt.status = 'resolved';
          cmt.resolved_at = Math.floor(Date.now() / 1000);
          cmt.resolved_by = resolvedBy || 'user';
          const openCount = this.documentComments.filter(
            c => c.file_path === cmt.file_path && c.status === 'open'
          ).length;
          this.setDocumentOpenCommentCount(cmt.file_path, openCount);
        }
      } catch (err) {
        console.error('[projectExplorerStore] Failed to resolve document comment:', err);
      }
    },

    applyDocumentCommentResolved(payload: DocumentCommentResolvedPayload) {
      if (!payload?.comment_id) return;
      const cmt = this.documentComments.find(c => c.id === payload.comment_id);
      if (cmt) {
        cmt.status = 'resolved';
        cmt.resolved_at = payload.resolved_at || Math.floor(Date.now() / 1000);
        cmt.resolved_by = payload.resolved_by || 'agent';
        const openCount = this.documentComments.filter(
          c => c.file_path === cmt.file_path && c.status === 'open'
        ).length;
        this.setDocumentOpenCommentCount(cmt.file_path, openCount);
      }
    },

    async deleteDocumentComment(commentId: string) {
      try {
        const target = this.documentComments.find(c => c.id === commentId);
        await daemonCall('delete_document_comment', { commentId });
        this.documentComments = this.documentComments.filter(c => c.id !== commentId);
        if (target) {
          const openCount = this.documentComments.filter(
            c => c.file_path === target.file_path && c.status === 'open'
          ).length;
          this.setDocumentOpenCommentCount(target.file_path, openCount);
        }
      } catch (err) {
        console.error('[projectExplorerStore] Failed to delete document comment:', err);
      }
    },

    attachFileToChat(file: AttachedReviewFile) {
      if (!this.attachedReviewFiles.some(f => f.rel_path === file.rel_path)) {
        this.attachedReviewFiles.push(file);
      }
    },

    async pasteClipboardImage(projectPath: string): Promise<boolean> {
      try {
        const res = await daemonCall('paste_clipboard_image', { projectPath });
        if (res) {
          this.attachFileToChat({
            rel_path: res.rel_path,
            name: res.name,
            doc_type: res.doc_type,
            is_external: res.is_external,
            size_bytes: res.size_bytes,
          });
          return true;
        }
      } catch (err) {
        console.error('[projectExplorerStore] Failed to read clipboard image:', err);
      }
      return false;
    },

    async attachClipboardImage(projectPath: string, imageBase64: string, extension = 'png') {
      try {
        const res = await daemonCall('save_clipboard_image', {
          projectPath,
          imageBase64,
          extension,
        });
        this.attachFileToChat({
          rel_path: res.rel_path,
          name: res.name,
          doc_type: 'image',
          is_external: true,
          preview_url: imageBase64,
          size_bytes: res.size_bytes,
        });
      } catch (err) {
        console.error('[projectExplorerStore] Failed to save clipboard image:', err);
      }
    },

    async attachFileByPath(projectPath: string, sourceAbsolutePath: string) {
      try {
        const res = await daemonCall('attach_file_to_project', {
          projectPath,
          sourceAbsolutePath,
        });
        this.attachFileToChat({
          rel_path: res.rel_path,
          name: res.name,
          doc_type: res.doc_type,
          is_external: res.is_external,
          size_bytes: res.size_bytes,
        });
      } catch (err) {
        console.error('[projectExplorerStore] Failed to attach file by path:', err);
      }
    },

    async openAndAttachFileDialog(projectPath: string) {
      try {
        const selected = await open({
          multiple: true,
          directory: false,
          title: 'Select File(s) to Attach',
        });
        if (!selected) return;
        const paths = Array.isArray(selected) ? selected : [selected];
        for (const filePath of paths) {
          if (typeof filePath === 'string') {
            await this.attachFileByPath(projectPath, filePath);
          }
        }
      } catch (err) {
        console.error('[projectExplorerStore] Failed to open file dialog:', err);
      }
    },

    detachFileFromChat(relPath: string) {
      this.attachedReviewFiles = this.attachedReviewFiles.filter(f => f.rel_path !== relPath);
    },

    clearAttachedReviewFiles() {
      this.attachedReviewFiles = [];
    },

    setExplorerClipboard(items: string[], operation: 'copy' | 'cut') {
      this.explorerClipboard = { items, operation };
    },

    clearExplorerClipboard() {
      this.explorerClipboard = null;
    },

    async createProjectFolder(projectPath: string, folderRelPath: string) {
      if (!projectPath || !folderRelPath.trim()) return;
      try {
        await daemonCall('create_project_directory', {
          projectPath,
          relPath: folderRelPath.trim(),
        });
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to create folder:', err);
        throw err;
      }
    },

    async createProjectFile(projectPath: string, fileRelPath: string) {
      if (!projectPath || !fileRelPath.trim()) return;
      try {
        await daemonCall('create_project_file', {
          projectPath,
          relPath: fileRelPath.trim(),
        });
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to create file:', err);
        throw err;
      }
    },

    async renameProjectItem(projectPath: string, oldRelPath: string, newRelPath: string) {
      if (!projectPath || !oldRelPath || !newRelPath.trim() || oldRelPath === newRelPath) return;
      try {
        await daemonCall('rename_project_item', {
          projectPath,
          oldRelPath,
          newRelPath: newRelPath.trim(),
        });
        const trimmedNew = newRelPath.trim();
        const newName = trimmedNew.split('/').pop() || trimmedNew;
        const openTab = this.openDocuments.find(d => d.rel_path === oldRelPath);
        if (openTab) {
          openTab.rel_path = trimmedNew;
          openTab.name = newName;
        }
        if (this.selectedDocument && this.selectedDocument.rel_path === oldRelPath) {
          this.selectedDocument.rel_path = trimmedNew;
          this.selectedDocument.name = newName;
        }
        if (this.unsavedDrafts[oldRelPath] !== undefined) {
          this.unsavedDrafts[trimmedNew] = this.unsavedDrafts[oldRelPath];
          delete this.unsavedDrafts[oldRelPath];
        }
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to rename item:', err);
        throw err;
      }
    },

    async deleteProjectItems(projectPath: string, relPaths: string[]) {
      if (!projectPath || relPaths.length === 0) return;
      try {
        await daemonCall('delete_project_items', {
          projectPath,
          relPaths,
        });
        this.openDocuments = this.openDocuments.filter(
          d => !relPaths.some(p => d.rel_path === p || d.rel_path.startsWith(p + '/'))
        );
        if (
          this.selectedDocument &&
          relPaths.some(
            p => this.selectedDocument?.rel_path === p || this.selectedDocument?.rel_path.startsWith(p + '/')
          )
        ) {
          this.selectedDocument = this.openDocuments[0] ?? null;
        }
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to delete items:', err);
        throw err;
      }
    },

    async copyProjectItems(projectPath: string, srcRelPaths: string[], destFolderRel: string) {
      if (!projectPath || srcRelPaths.length === 0) return;
      try {
        await daemonCall('copy_project_items', {
          projectPath,
          srcRelPaths,
          destFolderRel,
        });
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to copy items:', err);
        throw err;
      }
    },

    async moveProjectItems(projectPath: string, srcRelPaths: string[], destFolderRel: string) {
      if (!projectPath || srcRelPaths.length === 0) return;
      try {
        await daemonCall('move_project_items', {
          projectPath,
          srcRelPaths,
          destFolderRel,
        });
        await this.fetchProjectDocuments(projectPath);
      } catch (err) {
        console.error('[projectExplorerStore] Failed to move items:', err);
        throw err;
      }
    },

    async refreshCurrentDocument(projectPath: string) {
      if (!projectPath) return;
      if (this.selectedDocument) {
        try {
          const doc = await daemonCall('read_project_document', {
            projectPath,
            relPath: this.selectedDocument.rel_path,
          });
          if (doc) {
            this.upsertOpenDocument(doc);
            this.selectedDocument = doc;
          }
        } catch (err) {
          console.warn('[projectExplorerStore] Failed to refresh current document:', err);
        }
      } else {
        const dialogueStore = useDialogueStore();
        if (dialogueStore.activeArtifact?.filePath) {
          await dialogueStore.refreshActiveArtifact(projectPath);
        }
      }
    },
  },
});
