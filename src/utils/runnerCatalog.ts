// Runner catalog of the UI: the runner manifests (`templates/runners/*.toml`) read through
// `list_agent_runners`. They are the only source of the runner → models → efforts relation;
// no component declares, picks or substitutes models on its own (Spec 10 §5).
import { computed, ref, watch, type Ref } from 'vue';
import type { RunnerModel, RunnerSchema } from '../types';
import type { ConfiguredAgentPair } from '../types/generated/ConfiguredAgentPair';
import { daemonCall } from './daemonClient';

const runners: Ref<RunnerSchema[]> = ref([]);
const loadError: Ref<string | null> = ref(null);
let loading: Promise<RunnerSchema[]> | null = null;

/** Loads the manifests once per app session; a failure is kept in `loadError` and rethrown. */
export function loadRunnerCatalog(): Promise<RunnerSchema[]> {
  if (!loading) {
    loading = daemonCall('list_agent_runners', {})
      .then((list) => {
        runners.value = list;
        loadError.value = null;
        return list;
      })
      .catch((e: unknown) => {
        loading = null;
        loadError.value = String(e);
        throw e;
      });
  }
  return loading;
}

export function findRunner(runnerId: string): RunnerSchema | undefined {
  return runners.value.find((r) => r.id === runnerId);
}

/** Models the runner's manifest declares (empty for an unknown runner). */
export function modelsOf(runnerId: string): RunnerModel[] {
  return findRunner(runnerId)?.models ?? [];
}

export function findModel(runnerId: string, modelId: string): RunnerModel | undefined {
  return modelsOf(runnerId).find((m) => m.id === modelId);
}

/** Efforts the manifest declares for the model (empty for a model without efforts). */
export function effortsOf(runnerId: string, modelId: string): string[] {
  return findModel(runnerId, modelId)?.effort_levels ?? [];
}

/** True when the pair (and the effort, when given) is declared by the runner's manifest. */
export function isDeclaredPair(runnerId: string, modelId: string, effort?: string | null): boolean {
  const model = findModel(runnerId, modelId);
  if (!model) return false;
  return !effort || model.effort_levels.includes(effort);
}

export function useRunnerCatalog() {
  return { runners, loadError, loadRunnerCatalog, findRunner, modelsOf, findModel, effortsOf, isDeclaredPair };
}

/**
 * Runner → model → effort selects bound to the manifests. Changing the runner clears a model it
 * does not declare (the user picks one of the new runner's); changing the model clears an effort
 * it does not declare ('' = the model's default effort).
 */
export function useAgentPairPicker() {
  const runner = ref('');
  const model = ref('');
  const effort = ref('');

  const runnerOptions = computed(() => runners.value.map((r) => ({ label: `${r.id} (${r.name})`, value: r.id })));
  const modelOptions = computed(() => [
    { label: 'Select a model', value: '' },
    ...modelsOf(runner.value).map((m) => ({ label: m.label, value: m.id })),
  ]);
  const currentModel = computed(() => findModel(runner.value, model.value));
  const effortOptions = computed(() => {
    const m = currentModel.value;
    if (!m || m.effort_levels.length === 0) return [];
    return [{ label: `Model default (${m.default_effort})`, value: '' }, ...m.effort_levels.map((e) => ({ label: e, value: e }))];
  });
  const isValid = computed(() => isDeclaredPair(runner.value, model.value, effort.value || null));

  watch(runner, (newRunner) => {
    if (runners.value.length > 0 && model.value && !findModel(newRunner, model.value)) {
      model.value = '';
    }
  });
  watch(currentModel, (m) => {
    if (runners.value.length > 0 && effort.value && !m?.effort_levels.includes(effort.value)) {
      effort.value = '';
    }
  });

  /** Loads the catalog and preselects a configured pair; an invalid configuration rejects. */
  async function loadConfiguredPair(pair: ConfiguredAgentPair) {
    await loadRunnerCatalog();
    const configured = await daemonCall('get_configured_agent_pair', { pair });
    runner.value = configured.runner;
    model.value = configured.model;
    effort.value = configured.effort ?? '';
  }

  return { runner, model, effort, runnerOptions, modelOptions, effortOptions, currentModel, isValid, loadConfiguredPair };
}
