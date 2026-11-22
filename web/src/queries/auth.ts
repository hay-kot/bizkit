import { useMutation, useQuery } from '@pinia/colada'
import { type MaybeRefOrGetter, toValue } from 'vue'

import {
  authSignInMutation,
  authSignOutMutation,
  authVerifyMutation,
  meGetQuery,
  platformMetaQuery,
} from '@/api/gen/@pinia/colada.gen'

import { useInvalidate } from './keys'

/** The signed-in user. Pass `enabled: false` where signed-out visitors land. */
export function useMe(enabled: MaybeRefOrGetter<boolean> = true) {
  return useQuery(() => ({ ...meGetQuery(), enabled: toValue(enabled) }))
}

export function useMeta() {
  return useQuery(platformMetaQuery())
}

export function useSignIn() {
  return useMutation(authSignInMutation())
}

export function useVerify() {
  const invalidate = useInvalidate()
  return useMutation({
    ...authVerifyMutation(),
    onSettled: () => invalidate('meGet', 'workspacesList'),
  })
}

export function useSignOut() {
  const invalidate = useInvalidate()
  return useMutation({ ...authSignOutMutation(), onSettled: () => invalidate('meGet') })
}
