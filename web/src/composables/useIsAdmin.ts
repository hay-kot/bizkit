import type { MaybeRefOrGetter } from 'vue'
import { computed, toValue } from 'vue'

import { useMe } from '@/queries/auth'

/** Whether the signed-in user is an admin of the workspace. */
export function useIsAdmin(workspaceId: MaybeRefOrGetter<string>) {
  const { data: me } = useMe()
  return computed(
    () =>
      me.value?.memberships.find((m) => m.workspace.id === toValue(workspaceId))?.role === 'admin',
  )
}
