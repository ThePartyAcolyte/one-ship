<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { Calendar, ChevronLeft, ChevronRight, X } from '@lucide/vue';

const props = withDefaults(
  defineProps<{
    modelValue?: string;
    placeholder?: string;
    disabled?: boolean;
  }>(),
  {
    modelValue: '',
    placeholder: 'YYYY-MM-DD',
    disabled: false,
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const isOpen = ref(false);
const triggerRef = ref<HTMLElement | null>(null);
const popoverRef = ref<HTMLElement | null>(null);
const popoverStyle = ref<Record<string, string>>({});

const todayIso = computed(() => formatIso(new Date()));
const viewYear = ref(new Date().getFullYear());
const viewMonth = ref(new Date().getMonth()); // 0-indexed

function formatIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function parseIso(raw: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(raw.trim());
  if (!m) return null;
  const y = Number(m[1]);
  const mo = Number(m[2]) - 1;
  const d = Number(m[3]);
  const dt = new Date(y, mo, d);
  if (dt.getFullYear() !== y || dt.getMonth() !== mo || dt.getDate() !== d) return null;
  return dt;
}

const monthLabel = computed(() => {
  const d = new Date(viewYear.value, viewMonth.value, 1);
  return d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }).toUpperCase();
});

interface CalendarCell {
  iso: string;
  day: number;
  inMonth: boolean;
}

const calendarDays = computed<CalendarCell[]>(() => {
  const firstOfMonth = new Date(viewYear.value, viewMonth.value, 1);
  // Monday-first grid: Mon=0..Sun=6
  const startOffset = (firstOfMonth.getDay() + 6) % 7;
  const startDate = new Date(viewYear.value, viewMonth.value, 1 - startOffset);
  const cells: CalendarCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(startDate.getFullYear(), startDate.getMonth(), startDate.getDate() + i);
    cells.push({
      iso: formatIso(d),
      day: d.getDate(),
      inMonth: d.getMonth() === viewMonth.value,
    });
  }
  return cells;
});

function prevMonth() {
  if (viewMonth.value === 0) {
    viewMonth.value = 11;
    viewYear.value -= 1;
  } else {
    viewMonth.value -= 1;
  }
}

function nextMonth() {
  if (viewMonth.value === 11) {
    viewMonth.value = 0;
    viewYear.value += 1;
  } else {
    viewMonth.value += 1;
  }
}

async function openPicker() {
  if (props.disabled) return;
  const parsed = props.modelValue ? parseIso(props.modelValue) : null;
  const anchor = parsed ?? new Date();
  viewYear.value = anchor.getFullYear();
  viewMonth.value = anchor.getMonth();
  isOpen.value = true;
  await nextTick();
  positionPopover();
}

function togglePicker() {
  if (isOpen.value) {
    isOpen.value = false;
  } else {
    void openPicker();
  }
}

function positionPopover() {
  if (!triggerRef.value) return;
  const rect = triggerRef.value.getBoundingClientRect();
  const popWidth = popoverRef.value?.offsetWidth ?? 256;
  const popHeight = popoverRef.value?.offsetHeight ?? 268;
  const margin = 8;
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  let top = rect.bottom + 4;
  if (top + popHeight > vh - margin && rect.top - 4 - popHeight >= margin) {
    top = rect.top - 4 - popHeight;
  }
  top = Math.max(margin, Math.min(top, vh - popHeight - margin));

  let left = rect.left;
  left = Math.max(margin, Math.min(left, vw - popWidth - margin));

  popoverStyle.value = {
    top: `${Math.round(top)}px`,
    left: `${Math.round(left)}px`,
  };
}

function selectDay(iso: string) {
  emit('update:modelValue', iso);
  isOpen.value = false;
}

function selectToday() {
  selectDay(todayIso.value);
}

function clearDate(e: Event) {
  e.stopPropagation();
  emit('update:modelValue', '');
  isOpen.value = false;
}

function handleDocumentClick(e: MouseEvent) {
  if (!isOpen.value) return;
  const target = e.target as Node | null;
  if (!target) return;
  if (triggerRef.value?.contains(target) || popoverRef.value?.contains(target)) return;
  isOpen.value = false;
}

function handleKeydown(e: KeyboardEvent) {
  if (isOpen.value && e.key === 'Escape') {
    e.stopPropagation();
    isOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentClick);
  document.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentClick);
  document.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div ref="triggerRef" class="relative w-full">
    <button
      type="button"
      :disabled="disabled"
      @click="togglePicker"
      class="w-full h-[32px] bg-[var(--ui-bg-base)] border px-2.5 py-1.5 text-xs font-mono flex items-center justify-between gap-2 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
      :class="isOpen ? 'border-[var(--accent-color)] text-[var(--fg-color)]' : 'border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)]/60'"
    >
      <span class="flex items-center gap-2 min-w-0 truncate">
        <Calendar :size="12" class="text-[var(--dim-color)] shrink-0" />
        <span :class="modelValue ? 'text-[var(--fg-color)]' : 'text-[var(--dim-color)]/60'">
          {{ modelValue || placeholder }}
        </span>
      </span>
      <span
        v-if="modelValue && !disabled"
        role="button"
        tabindex="-1"
        @click="clearDate"
        class="text-[var(--dim-color)] hover:text-[var(--status-error-color)] p-0.5 shrink-0"
      >
        <X :size="11" />
      </span>
    </button>

    <Teleport to="body">
      <div
        v-if="isOpen"
        ref="popoverRef"
        :style="popoverStyle"
        class="fixed z-[99999] w-64 bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl p-2 font-mono select-none"
      >
        <!-- Month header -->
        <div class="flex items-center justify-between gap-1 pb-2 mb-1.5 border-b border-[var(--border-color)]">
          <button
            type="button"
            @click="prevMonth"
            class="ui-icon-btn w-6 h-6 flex items-center justify-center border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] cursor-pointer"
          >
            <ChevronLeft :size="12" />
          </button>
          <span class="text-[10px] font-bold tracking-wider text-[var(--fg-color)]">
            {{ monthLabel }}
          </span>
          <button
            type="button"
            @click="nextMonth"
            class="ui-icon-btn w-6 h-6 flex items-center justify-center border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] cursor-pointer"
          >
            <ChevronRight :size="12" />
          </button>
        </div>

        <!-- Weekday headers -->
        <div class="grid grid-cols-7 gap-0.5 mb-1 text-center text-[8px] uppercase text-[var(--dim-color)] font-bold">
          <span>Mo</span>
          <span>Tu</span>
          <span>We</span>
          <span>Th</span>
          <span>Fr</span>
          <span>Sa</span>
          <span>Su</span>
        </div>

        <!-- Day cells -->
        <div class="grid grid-cols-7 gap-0.5">
          <button
            v-for="cell in calendarDays"
            :key="cell.iso"
            type="button"
            @click="selectDay(cell.iso)"
            class="h-6 text-[10px] flex items-center justify-center border transition-colors cursor-pointer"
            :class="[
              cell.iso === modelValue
                ? 'bg-[var(--accent-color)] text-[var(--bg-color)] border-[var(--accent-color)] font-bold'
                : cell.iso === todayIso
                  ? 'border-[var(--accent-color)]/60 text-[var(--accent-color)] hover:bg-[var(--ui-bg-card)]'
                  : cell.inMonth
                    ? 'border-transparent text-[var(--fg-color)] hover:bg-[var(--ui-bg-card)] hover:border-[var(--border-color)]'
                    : 'border-transparent text-[var(--dim-color)]/45 hover:bg-[var(--ui-bg-card)]/60'
            ]"
          >
            {{ cell.day }}
          </button>
        </div>

        <!-- Footer -->
        <div class="flex items-center justify-between gap-2 pt-2 mt-1.5 border-t border-[var(--border-color)]">
          <button
            type="button"
            @click="selectToday"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] cursor-pointer"
          >
            Today
          </button>
          <button
            type="button"
            @click="clearDate"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--status-error-color)] hover:border-[var(--status-error-color)] cursor-pointer"
          >
            Clear
          </button>
        </div>
      </div>
    </Teleport>
  </div>
</template>
