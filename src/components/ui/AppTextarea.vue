<script setup lang="ts">
import { ref, nextTick, watch, onMounted } from 'vue';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    rows?: number;
    maxRows?: number;
    autoGrow?: boolean;
    autoScroll?: boolean;
    placeholder?: string;
    disabled?: boolean;
  }>(),
  {
    rows: 3,
    maxRows: 10,
    autoGrow: false,
    autoScroll: false,
    placeholder: '',
    disabled: false,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'paste', event: ClipboardEvent): void;
  (e: 'click', event: MouseEvent): void;
  (e: 'focus', event: FocusEvent): void;
  (e: 'blur', event: FocusEvent): void;
  (e: 'keydown', event: KeyboardEvent): void;
}>();

const textareaEl = ref<HTMLTextAreaElement | null>(null);

function adjustHeight() {
  if (!props.autoGrow || !textareaEl.value) return;
  const el = textareaEl.value;
  el.style.height = 'auto';

  const computed = window.getComputedStyle(el);
  const lineHeight = parseFloat(computed.lineHeight) || 16;
  const paddingTop = parseFloat(computed.paddingTop) || 8;
  const paddingBottom = parseFloat(computed.paddingBottom) || 8;
  const borderTop = parseFloat(computed.borderTopWidth) || 1;
  const borderBottom = parseFloat(computed.borderBottomWidth) || 1;

  const minHeight = (lineHeight * props.rows) + paddingTop + paddingBottom + borderTop + borderBottom;
  const maxHeight = (lineHeight * props.maxRows) + paddingTop + paddingBottom + borderTop + borderBottom;

  const contentHeight = el.scrollHeight + borderTop + borderBottom;
  const targetHeight = Math.max(minHeight, Math.min(contentHeight, maxHeight));

  el.style.height = `${targetHeight}px`;
  el.style.overflowX = 'hidden';

  if (contentHeight > maxHeight) {
    el.style.overflowY = 'auto';
  } else {
    el.style.overflowY = 'hidden';
  }
}

function scrollToBottom() {
  if (!textareaEl.value) return;
  textareaEl.value.scrollTop = textareaEl.value.scrollHeight;
}

function onInput(event: Event) {
  const target = event.target as HTMLTextAreaElement;
  emit('update:modelValue', target.value);
  if (props.autoGrow) {
    adjustHeight();
  }
  if (props.autoScroll) {
    scrollToBottom();
  }
}

function onPaste(event: ClipboardEvent) {
  emit('paste', event);
  if (props.autoGrow) {
    nextTick(adjustHeight);
  }
}

function insertAtCursor(text: string) {
  if (!textareaEl.value) return;
  const el = textareaEl.value;
  const start = el.selectionStart ?? el.value.length;
  const end = el.selectionEnd ?? el.value.length;
  const current = el.value;
  const next = current.substring(0, start) + text + current.substring(end);
  emit('update:modelValue', next);
  nextTick(() => {
    el.focus();
    el.selectionStart = el.selectionEnd = start + text.length;
    if (props.autoGrow) {
      adjustHeight();
    }
    if (props.autoScroll) {
      scrollToBottom();
    }
  });
}

watch(
  () => props.modelValue,
  () => {
    nextTick(() => {
      adjustHeight();
      if (props.autoScroll) {
        scrollToBottom();
      }
    });
  }
);

onMounted(() => {
  if (props.autoGrow) {
    nextTick(adjustHeight);
  }
});

defineExpose({
  textareaEl,
  insertAtCursor,
  adjustHeight,
  scrollToBottom,
  focus: () => textareaEl.value?.focus(),
});
</script>

<template>
  <textarea
    ref="textareaEl"
    :rows="rows"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    @input="onInput"
    @paste="onPaste"
    @click="emit('click', $event)"
    @focus="emit('focus', $event)"
    @blur="emit('blur', $event)"
    @keydown="emit('keydown', $event)"
    class="w-full bg-[var(--bg-color)]/60 border border-[var(--border-color)] p-2 text-xs text-[var(--fg-color)] focus:border-[var(--accent-color)] outline-none font-mono placeholder:text-[var(--dim-color)]/50 resize-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed overflow-x-hidden break-words whitespace-pre-wrap"
  />
</template>
