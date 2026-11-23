import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  extensionsHooksListQuery,
  extensionsRunsListQuery,
  extensionsScriptsCreateMutation,
  extensionsScriptsDeleteMutation,
  extensionsScriptsGetQuery,
  extensionsScriptsListQuery,
  extensionsScriptsTestMutation,
  extensionsScriptsUpdateMutation,
  extensionsSecretsDeleteMutation,
  extensionsSecretsListQuery,
  extensionsSecretsPutMutation,
} from '@/api/gen/@pinia/colada.gen'
import type {
  Capability,
  Hook,
  Run,
  RunStatus,
  Script,
  ScriptSummary,
  Secret,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { Capability, Hook, Run, RunStatus, Script, ScriptSummary, Secret }

const SCRIPT_QUERIES = ['extensionsScriptsList', 'extensionsScriptsGet']
const RUN_QUERIES = ['extensionsRunsList']
const SECRET_QUERIES = ['extensionsSecretsList']

export function useHooks(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => extensionsHooksListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useScripts(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    extensionsScriptsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useScript(
  workspaceId: MaybeRefOrGetter<string>,
  scriptId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    extensionsScriptsGetQuery({
      path: { workspace_id: toValue(workspaceId), script_id: toValue(scriptId) },
    }),
  )
}

export function useCreateScript() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsScriptsCreateMutation(),
    onSettled: () => invalidate(...SCRIPT_QUERIES),
  })
}

export function useUpdateScript() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsScriptsUpdateMutation(),
    onSettled: () => invalidate(...SCRIPT_QUERIES),
  })
}

export function useDeleteScript() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsScriptsDeleteMutation(),
    onSettled: () => invalidate(...SCRIPT_QUERIES),
  })
}

export function useTestScript() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsScriptsTestMutation(),
    onSettled: () => invalidate(...RUN_QUERIES),
  })
}

export function useRuns(workspaceId: MaybeRefOrGetter<string>, scriptId: MaybeRefOrGetter<string>) {
  return useQuery(() =>
    extensionsRunsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { script_id: toValue(scriptId) },
    }),
  )
}

export function useSecrets(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    extensionsSecretsListQuery({
      path: { workspace_id: toValue(workspaceId), project_id: toValue(projectId) },
    }),
  )
}

export function usePutSecret() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsSecretsPutMutation(),
    onSettled: () => invalidate(...SECRET_QUERIES),
  })
}

export function useDeleteSecret() {
  const invalidate = useInvalidate()
  return useMutation({
    ...extensionsSecretsDeleteMutation(),
    onSettled: () => invalidate(...SECRET_QUERIES),
  })
}
