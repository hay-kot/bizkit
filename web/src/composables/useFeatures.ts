import type { MaybeRefOrGetter } from 'vue'
import { computed, toValue } from 'vue'

import { useWorkspaceFeatures } from '@/queries/workspaces'

/**
 * The modules a workspace turned off. Until the list loads nothing counts as
 * off, so a slow request never hides the whole sidebar.
 */
export function useFeatures(workspaceId: MaybeRefOrGetter<string>) {
  const { data, error } = useWorkspaceFeatures(() => toValue(workspaceId))
  const off = computed(() =>
    (data.value?.items ?? []).filter((feature) => !feature.enabled).map((feature) => feature.name),
  )
  // Settled either way: on an error every feature counts as on, and the API
  // still refuses what is off.
  const loaded = computed(() => data.value !== undefined || error.value !== null)
  /** True unless `module` is a feature the workspace turned off. Core is always on. */
  const isOn = (module: string | undefined) => !module || !off.value.includes(module)
  return { off, loaded, isOn }
}
