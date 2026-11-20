<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue';
import { AppTooltip } from './ui';
import type { ProjectRecord } from '../types';

const props = defineProps<{
  projects: ProjectRecord[];
  scrollContainerRef: HTMLElement | null;
}>();

const activeProjectId = ref<number | null>(null);
let rafId: number | null = null;

function scrollToProject(project: ProjectRecord) {
  const el = document.getElementById(`project-card-${project.id}`);
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
}

function calculateActiveProject() {
  if (!props.scrollContainerRef || !props.projects.length) return;
  const container = props.scrollContainerRef;
  const { scrollTop, scrollHeight, clientHeight } = container;

  // Edge 1: Top of container
  if (scrollTop <= 10) {
    activeProjectId.value = props.projects[0]?.id ?? null;
    return;
  }

  // Edge 2: Bottom of container reached
  if (scrollTop + clientHeight >= scrollHeight - 20) {
    activeProjectId.value = props.projects[props.projects.length - 1]?.id ?? null;
    return;
  }

  const containerRect = container.getBoundingClientRect();
  // Focal reading line positioned at 120px down from container top
  const focalPoint = containerRect.top + 120;

  let fallbackProject = props.projects[0];
  let minDistance = Infinity;

  for (const project of props.projects) {
    const el = document.getElementById(`project-card-${project.id}`);
    if (!el) continue;
    const rect = el.getBoundingClientRect();

    // Card spans across the focal line
    if (rect.top <= focalPoint && rect.bottom >= focalPoint) {
      activeProjectId.value = project.id;
      return;
    }

    const dist = Math.abs(rect.top - focalPoint);
    if (dist < minDistance) {
      minDistance = dist;
      fallbackProject = project;
    }
  }

  if (fallbackProject) {
    activeProjectId.value = fallbackProject.id;
  }
}

function onScroll() {
  if (rafId !== null) return;
  rafId = requestAnimationFrame(() => {
    calculateActiveProject();
    rafId = null;
  });
}

function bindScrollListener() {
  if (!props.scrollContainerRef) return;
  props.scrollContainerRef.removeEventListener('scroll', onScroll);
  props.scrollContainerRef.addEventListener('scroll', onScroll, { passive: true });
  nextTick(calculateActiveProject);
}

watch(() => [props.projects, props.scrollContainerRef], () => {
  bindScrollListener();
}, { immediate: true });

onMounted(() => {
  bindScrollListener();
  window.addEventListener('resize', onScroll, { passive: true });
});

onUnmounted(() => {
  if (props.scrollContainerRef) {
    props.scrollContainerRef.removeEventListener('scroll', onScroll);
  }
  window.removeEventListener('resize', onScroll);
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
  }
});
</script>

<template>
  <div
    v-if="projects.length > 1"
    class="fixed left-2 top-1/2 -translate-y-1/2 z-40 flex flex-col items-center gap-2 py-2"
  >
    <AppTooltip
      v-for="project in projects"
      :key="project.id"
      :text="project.name"
      position="right"
    >
      <button
        class="w-1.5 rounded-full transition-all duration-300 cursor-pointer focus:outline-none"
        :class="[
          activeProjectId === project.id
            ? 'h-4 bg-[var(--accent-color)]'
            : 'h-1.5 bg-[var(--border-color)] hover:bg-[var(--dim-color)] opacity-50 hover:opacity-100'
        ]"
        @click="scrollToProject(project)"
      />
    </AppTooltip>
  </div>
</template>
