import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  apiKeysCreateMutation,
  apiKeysListQuery,
  apiKeysRevokeMutation,
} from '@/api/gen/@pinia/colada.gen'
import type { ApiKey, CreatedApiKey, ScopeArea } from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { ApiKey, CreatedApiKey, ScopeArea }

export function useApiKeys(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => apiKeysListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useCreateApiKey() {
  const invalidate = useInvalidate()
  return useMutation({ ...apiKeysCreateMutation(), onSettled: () => invalidate('apiKeysList') })
}

export function useRevokeApiKey() {
  const invalidate = useInvalidate()
  return useMutation({ ...apiKeysRevokeMutation(), onSettled: () => invalidate('apiKeysList') })
}
