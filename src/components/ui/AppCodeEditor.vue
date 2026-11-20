<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue';
import { basicSetup } from 'codemirror';
import { EditorView, keymap } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { indentWithTab } from '@codemirror/commands';
import { language, syntaxTree, syntaxTreeAvailable, StreamLanguage } from '@codemirror/language';
import { linter, lintGutter, type Diagnostic } from '@codemirror/lint';
import { rust } from '@codemirror/lang-rust';
import { javascript } from '@codemirror/lang-javascript';
import { vue } from '@codemirror/lang-vue';
import { svelte } from '@replit/codemirror-lang-svelte';
import { astro } from '@fazelstudio/codemirror-lang-astro';
import { markdown } from '@codemirror/lang-markdown';
import { json } from '@codemirror/lang-json';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { python } from '@codemirror/lang-python';
import { cpp } from '@codemirror/lang-cpp';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { go } from '@codemirror/legacy-modes/mode/go';
import { dockerFile } from '@codemirror/legacy-modes/mode/dockerfile';
import { standardSQL } from '@codemirror/legacy-modes/mode/sql';
import { xml } from '@codemirror/legacy-modes/mode/xml';
import { ruby } from '@codemirror/legacy-modes/mode/ruby';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { diff } from '@codemirror/legacy-modes/mode/diff';
import { oneDark } from '@codemirror/theme-one-dark';

export interface EditorCursorInfo {
  line: number;
  col: number;
  selectionStartLine: number;
  selectionEndLine: number;
  hasSelection: boolean;
}

export interface EditorContextMenuPayload {
  x: number;
  y: number;
  selectedText: string;
  startLine: number;
  endLine: number;
  wordAtCursor: string;
  cursorLine: number;
}

export interface EditorDiagnosticsSummary {
  errorCount: number;
  warningCount: number;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    language?: string;
    filePath?: string;
    readonly?: boolean;
    lineWrapping?: boolean;
  }>(),
  {
    language: 'text',
    readonly: false,
    lineWrapping: true,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'save'): void;
  (e: 'change', value: string): void;
  (e: 'cursor-change', payload: EditorCursorInfo): void;
  (e: 'diagnostics-change', payload: EditorDiagnosticsSummary): void;
  (e: 'editor-contextmenu', payload: EditorContextMenuPayload): void;
  (e: 'goto-definition', payload: { symbol: string; line: number }): void;
  (e: 'toggle-outline'): void;
}>();

const editorContainerRef = ref<HTMLElement | null>(null);
let view: EditorView | null = null;

const langCompartment = new Compartment();
const lintCompartment = new Compartment();
const readonlyCompartment = new Compartment();
const editableCompartment = new Compartment();
const lineWrappingCompartment = new Compartment();

function resolveExtension(path?: string): string {
  if (!path) return '';
  const base = path.split('/').pop()?.toLowerCase() ?? '';
  if (base === 'dockerfile') return 'dockerfile';
  const dotIdx = base.lastIndexOf('.');
  if (dotIdx === -1) return '';
  return base.slice(dotIdx + 1);
}

function getLanguageExtension(lang?: string, path?: string) {
  const effectiveLang = (lang || '').toLowerCase();
  const ext = resolveExtension(path);

  if (effectiveLang === 'rust' || ext === 'rs') {
    return rust();
  }
  if (effectiveLang === 'vue' || ext === 'vue') {
    return vue();
  }
  if (effectiveLang === 'svelte' || ext === 'svelte') {
    return svelte();
  }
  if (effectiveLang === 'astro' || ext === 'astro') {
    return astro();
  }
  if (['tsx'].includes(effectiveLang) || ext === 'tsx') {
    return javascript({ typescript: true, jsx: true });
  }
  if (
    ['typescript', 'ts'].includes(effectiveLang) ||
    ['ts', 'mts', 'cts'].includes(ext)
  ) {
    return javascript({ typescript: true, jsx: false });
  }
  if (
    ['javascript', 'js', 'jsx'].includes(effectiveLang) ||
    ['js', 'mjs', 'cjs', 'jsx'].includes(ext)
  ) {
    return javascript({ typescript: false, jsx: true });
  }
  if (['c', 'cpp', 'c++', 'h', 'hpp'].includes(effectiveLang) || ['c', 'h', 'cpp', 'cc', 'cxx', 'hpp', 'hxx', 'ino'].includes(ext)) {
    return cpp();
  }
  if (['markdown', 'md'].includes(effectiveLang) || ['md', 'markdown', 'mdx'].includes(ext)) {
    return markdown();
  }
  if (effectiveLang === 'json' || ['json', 'jsonc'].includes(ext)) {
    return json();
  }
  if (effectiveLang === 'html' || ['html', 'htm'].includes(ext)) {
    return html();
  }
  if (['css', 'scss', 'less'].includes(effectiveLang) || ['css', 'scss', 'less'].includes(ext)) {
    return css();
  }
  if (['python', 'py'].includes(effectiveLang) || ['py', 'pyi'].includes(ext)) {
    return python();
  }
  if (effectiveLang === 'toml' || ext === 'toml') {
    return StreamLanguage.define(toml);
  }
  if (['bash', 'sh', 'zsh', 'shell'].includes(effectiveLang) || ['sh', 'bash', 'zsh', 'fish'].includes(ext)) {
    return StreamLanguage.define(shell);
  }
  if (['yaml', 'yml'].includes(effectiveLang) || ['yaml', 'yml'].includes(ext)) {
    return StreamLanguage.define(yaml);
  }
  if (effectiveLang === 'go' || ext === 'go') {
    return StreamLanguage.define(go);
  }
  if (effectiveLang === 'dockerfile' || ext === 'dockerfile') {
    return StreamLanguage.define(dockerFile);
  }
  if (effectiveLang === 'sql' || ext === 'sql') {
    return StreamLanguage.define(standardSQL);
  }
  if (effectiveLang === 'xml' || ['xml', 'svg', 'plist', 'kicad_sch', 'kicad_pcb'].includes(ext)) {
    return StreamLanguage.define(xml);
  }
  if (effectiveLang === 'ruby' || ext === 'rb') {
    return StreamLanguage.define(ruby);
  }
  if (['ini', 'properties', 'conf'].includes(effectiveLang) || ['ini', 'conf', 'cfg', 'properties', 'env'].includes(ext)) {
    return StreamLanguage.define(properties);
  }
  if (effectiveLang === 'diff' || ['diff', 'patch'].includes(ext)) {
    return StreamLanguage.define(diff);
  }
  return [];
}

function isJsonDocument(): boolean {
  const ext = resolveExtension(props.filePath);
  return (props.language || '').toLowerCase() === 'json' || ext === 'json';
}

function isLintableDocument(): boolean {
  const ext = resolveExtension(props.filePath);
  const lang = (props.language || '').toLowerCase();
  if (['md', 'markdown', 'mdx', 'txt', 'log', 'diff', 'patch', 'vue', 'svelte', 'astro'].includes(ext)) {
    return false;
  }
  if (['markdown', 'md', 'text', 'plaintext', 'vue', 'svelte', 'astro'].includes(lang) && !ext) {
    return false;
  }
  return [
    'rs', 'js', 'mjs', 'cjs', 'ts', 'mts', 'cts', 'jsx', 'tsx',
    'c', 'h', 'cpp', 'cc', 'cxx', 'hpp', 'hxx', 'ino',
    'py', 'pyi', 'json', 'html', 'htm', 'css', 'scss', 'less',
  ].includes(ext) || ['rust', 'javascript', 'typescript', 'python', 'json', 'html', 'css', 'c', 'cpp'].includes(lang);
}

function computeJsonLineColOffset(docText: string, errMessage: string): number {
  const posMatch = /position\s+(\d+)/i.exec(errMessage);
  if (posMatch) {
    const idx = Number.parseInt(posMatch[1], 10);
    if (Number.isFinite(idx)) {
      return Math.max(0, Math.min(docText.length, idx));
    }
  }
  const lineColMatch = /line\s+(\d+)\s+column\s+(\d+)/i.exec(errMessage);
  if (lineColMatch) {
    const targetLine = Math.max(1, Number.parseInt(lineColMatch[1], 10));
    const targetCol = Math.max(1, Number.parseInt(lineColMatch[2], 10));
    const lines = docText.split('\n');
    let offset = 0;
    for (let i = 0; i < Math.min(targetLine - 1, lines.length); i++) {
      offset += lines[i].length + 1;
    }
    return Math.min(docText.length, offset + Math.max(0, targetCol - 1));
  }
  return 0;
}

function runSyntaxLinter(targetView: EditorView): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const doc = targetView.state.doc;
  const text = doc.toString();

  if (!text.trim()) {
    emit('diagnostics-change', { errorCount: 0, warningCount: 0 });
    return diagnostics;
  }

  if (isJsonDocument()) {
    try {
      JSON.parse(text);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Invalid JSON syntax';
      const offset = computeJsonLineColOffset(text, msg);
      const from = Math.max(0, Math.min(doc.length, offset));
      const to = Math.min(doc.length, Math.max(from + 1, from + 1));
      diagnostics.push({
        from,
        to,
        severity: 'error',
        message: msg,
      });
    }
    emit('diagnostics-change', { errorCount: diagnostics.length, warningCount: 0 });
    return diagnostics;
  }

  const MAX_SYNTAX_DIAGNOSTICS_COUNT = 50;
  const activeLang = targetView.state.facet(language);
  const tree = syntaxTreeAvailable(targetView.state, doc.length)
    ? syntaxTree(targetView.state)
    : (activeLang ? activeLang.parser.parse(text) : syntaxTree(targetView.state));
  const seenOffsets = new Set<number>();
  const cursor = tree.cursor();
  do {
    if (cursor.type.isError) {
      const from = cursor.from;
      if (seenOffsets.has(from)) {
        continue;
      }
      seenOffsets.add(from);
      const to = Math.min(doc.length, Math.max(cursor.to, cursor.from + 1));
      const snippet = doc.sliceString(from, Math.min(doc.length, from + 24)).replace(/\s+/g, ' ').trim();
      diagnostics.push({
        from,
        to,
        severity: 'error',
        message: snippet ? `Syntax error near "${snippet}"` : 'Unexpected syntax token',
      });
      if (diagnostics.length >= MAX_SYNTAX_DIAGNOSTICS_COUNT) {
        break;
      }
    }
  } while (cursor.next());

  emit('diagnostics-change', { errorCount: diagnostics.length, warningCount: 0 });
  return diagnostics;
}

function buildLintExtension() {
  if (!isLintableDocument()) {
    emit('diagnostics-change', { errorCount: 0, warningCount: 0 });
    return [];
  }
  return [
    lintGutter(),
    linter(runSyntaxLinter, { delay: 350 }),
  ];
}

function extractIdentifierAtPos(state: EditorState, pos: number): string {
  const clamped = Math.max(0, Math.min(state.doc.length, pos));
  const line = state.doc.lineAt(clamped);
  const relOffset = clamped - line.from;
  const text = line.text;
  const isIdentChar = (ch: string) => /[A-Za-z0-9_]/.test(ch);

  let start = relOffset;
  while (start > 0 && isIdentChar(text[start - 1])) {
    start--;
  }
  let end = relOffset;
  while (end < text.length && isIdentChar(text[end])) {
    end++;
  }
  const token = text.slice(start, end);
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(token)) {
    return '';
  }
  return token;
}

function emitCursorSnapshot(state: EditorState) {
  const sel = state.selection.main;
  const headLine = state.doc.lineAt(sel.head);
  const fromLine = state.doc.lineAt(sel.from);
  const toLine = state.doc.lineAt(sel.to);
  emit('cursor-change', {
    line: headLine.number,
    col: sel.head - headLine.from + 1,
    selectionStartLine: fromLine.number,
    selectionEndLine: toLine.number,
    hasSelection: !sel.empty,
  });
}

const brutalistTheme = EditorView.theme(
  {
    '&': {
      backgroundColor: 'transparent',
      color: 'var(--fg-color)',
      fontFamily: 'var(--app-font-family), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: '11px',
      height: '100%',
    },
    '.cm-content': {
      caretColor: 'var(--accent-color)',
      lineHeight: '1.6',
      padding: '8px 0',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent-color)',
      borderLeftWidth: '2px',
    },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'color-mix(in srgb, var(--accent-color) 28%, transparent) !important',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--ui-bg-list)',
      color: 'var(--dim-color)',
      borderRight: '1px solid var(--border-color)',
      paddingRight: '6px',
      fontSize: '10px',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'color-mix(in srgb, var(--fg-color) 6%, transparent)',
      color: 'var(--accent-color)',
      fontWeight: 'bold',
    },
    '.cm-activeLine': {
      backgroundColor: 'color-mix(in srgb, var(--fg-color) 3%, transparent)',
    },
    '.cm-scroller': {
      overflow: 'auto',
      fontFamily: 'inherit',
    },
    '.cm-lintRange-error': {
      backgroundImage: 'none',
      textDecoration: 'underline wavy var(--status-error-color)',
      textUnderlineOffset: '3px',
    },
    '.cm-lintRange-warning': {
      backgroundImage: 'none',
      textDecoration: 'underline wavy var(--status-warn-color)',
      textUnderlineOffset: '3px',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--ui-bg-title)',
      border: '1px solid var(--border-color)',
      color: 'var(--fg-color)',
      fontFamily: 'var(--app-font-family), monospace',
      fontSize: '10px',
      borderRadius: '0',
    },
    '.cm-diagnostic': {
      padding: '4px 8px',
      borderLeft: '2px solid var(--status-error-color)',
    },
    '.cm-diagnostic-warning': {
      borderLeftColor: 'var(--status-warn-color)',
    },
  },
  { dark: true }
);

function initEditor() {
  if (!editorContainerRef.value) return;

  const editorKeymap = keymap.of([
    {
      key: 'Mod-s',
      run: () => {
        emit('save');
        return true;
      },
    },
    {
      key: 'F12',
      run: (targetView) => {
        const sel = targetView.state.selection.main;
        const symbol = extractIdentifierAtPos(targetView.state, sel.head);
        if (symbol) {
          const line = targetView.state.doc.lineAt(sel.head).number;
          emit('goto-definition', { symbol, line });
          return true;
        }
        return false;
      },
    },
    {
      key: 'Mod-Shift-o',
      run: () => {
        emit('toggle-outline');
        return true;
      },
    },
    indentWithTab,
  ]);

  const domHandlers = EditorView.domEventHandlers({
    contextmenu: (event, targetView) => {
      event.preventDefault();
      const state = targetView.state;
      const sel = state.selection.main;
      const clickPos = targetView.posAtCoords({ x: event.clientX, y: event.clientY });
      const activePos = clickPos ?? sel.head;
      const wordAtCursor = extractIdentifierAtPos(state, activePos);
      const cursorLine = state.doc.lineAt(activePos).number;

      const selectedText = !sel.empty ? state.sliceDoc(sel.from, sel.to) : '';
      const startLine = !sel.empty ? state.doc.lineAt(sel.from).number : cursorLine;
      const endLine = !sel.empty ? state.doc.lineAt(sel.to).number : cursorLine;

      emit('editor-contextmenu', {
        x: event.clientX,
        y: event.clientY,
        selectedText,
        startLine,
        endLine,
        wordAtCursor,
        cursorLine,
      });
      return true;
    },
    mousedown: (event, targetView) => {
      if ((event.ctrlKey || event.metaKey) && event.button === 0) {
        const clickPos = targetView.posAtCoords({ x: event.clientX, y: event.clientY });
        if (clickPos !== null) {
          const symbol = extractIdentifierAtPos(targetView.state, clickPos);
          if (symbol) {
            event.preventDefault();
            const line = targetView.state.doc.lineAt(clickPos).number;
            emit('goto-definition', { symbol, line });
            return true;
          }
        }
      }
      return false;
    },
  });

  const updateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged) {
      const value = update.state.doc.toString();
      emit('update:modelValue', value);
      emit('change', value);
    }
    if (update.selectionSet || update.docChanged) {
      emitCursorSnapshot(update.state);
    }
  });

  const state = EditorState.create({
    doc: props.modelValue,
    extensions: [
      basicSetup,
      oneDark,
      brutalistTheme,
      editorKeymap,
      domHandlers,
      updateListener,
      langCompartment.of(getLanguageExtension(props.language, props.filePath)),
      lintCompartment.of(buildLintExtension()),
      readonlyCompartment.of(EditorState.readOnly.of(props.readonly)),
      editableCompartment.of(EditorView.editable.of(!props.readonly)),
      lineWrappingCompartment.of(props.lineWrapping ? EditorView.lineWrapping : []),
    ],
  });

  view = new EditorView({
    state,
    parent: editorContainerRef.value,
  });

  emitCursorSnapshot(view.state);
}

function scrollToLine(lineNumber: number, shouldFocus = false) {
  if (!view) return;
  const totalLines = view.state.doc.lines;
  const clampedLine = Math.max(1, Math.min(totalLines, lineNumber));
  const line = view.state.doc.line(clampedLine);
  view.dispatch({
    selection: { anchor: line.from },
    effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
  });
  if (shouldFocus) {
    view.focus();
  }
}

function focus() {
  view?.focus();
}

function getSelectionContext() {
  if (!view) return null;
  const state = view.state;
  const sel = state.selection.main;
  const headLine = state.doc.lineAt(sel.head);
  return {
    selectedText: !sel.empty ? state.sliceDoc(sel.from, sel.to) : '',
    startLine: state.doc.lineAt(sel.from).number,
    endLine: state.doc.lineAt(sel.to).number,
    cursorLine: headLine.number,
    cursorCol: sel.head - headLine.from + 1,
    wordAtCursor: extractIdentifierAtPos(state, sel.head),
  };
}

defineExpose({
  scrollToLine,
  focus,
  getSelectionContext,
});

watch(
  () => props.modelValue,
  (newVal) => {
    if (view && newVal !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: newVal },
      });
    }
  }
);

watch(
  () => [props.language, props.filePath],
  () => {
    if (view) {
      view.dispatch({
        effects: [
          langCompartment.reconfigure(getLanguageExtension(props.language, props.filePath)),
          lintCompartment.reconfigure(buildLintExtension()),
        ],
      });
    }
  }
);

watch(
  () => props.readonly,
  (isReadOnly) => {
    if (view) {
      view.dispatch({
        effects: [
          readonlyCompartment.reconfigure(EditorState.readOnly.of(isReadOnly)),
          editableCompartment.reconfigure(EditorView.editable.of(!isReadOnly)),
        ],
      });
    }
  }
);

watch(
  () => props.lineWrapping,
  (wrap) => {
    if (view) {
      view.dispatch({
        effects: lineWrappingCompartment.reconfigure(wrap ? EditorView.lineWrapping : []),
      });
    }
  }
);

onMounted(() => {
  initEditor();
});

onBeforeUnmount(() => {
  if (view) {
    view.destroy();
    view = null;
  }
});
</script>

<template>
  <div ref="editorContainerRef" class="w-full h-full overflow-hidden font-mono text-[11px]" />
</template>

<style scoped>
:deep(.cm-editor) {
  height: 100%;
  outline: none !important;
}
</style>
