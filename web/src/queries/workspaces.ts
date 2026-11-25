import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  invitationsAcceptMutation,
  invitationsDeclineMutation,
  invitationsMineQuery,
  workspacesCreateMutation,
  workspacesFeaturesGetQuery,
  workspacesFeaturesPutMutation,
  workspacesGetQuery,
  workspacesUpdateMutation,
} from '@/api/gen/@pinia/colada.gen'
import type { Feature, Membership, MyInvitation, Workspace } from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { Feature, Membership, MyInvitation, Workspace }

export function useWorkspace(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => workspacesGetQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

/** The workspace's features (modules) and whether each is on. */
export function useWorkspaceFeatures(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() =>
    workspacesFeaturesGetQuery({ path: { workspace_id: toValue(workspaceId) } }),
  )
}

export function useUpdateWorkspaceFeatures() {
  const invalidate = useInvalidate()
  return useMutation({
    ...workspacesFeaturesPutMutation(),
    onSettled: () => invalidate('workspacesFeaturesGet'),
  })
}

export function useCreateWorkspace() {
  const invalidate = useInvalidate()
  return useMutation({
    ...workspacesCreateMutation(),
    onSettled: () => invalidate('meGet', 'workspacesList'),
  })
}

export function useRenameWorkspace() {
  const invalidate = useInvalidate()
  return useMutation({
    ...workspacesUpdateMutation(),
    onSettled: () => invalidate('meGet', 'workspacesGet'),
  })
}

export function useMyInvitations() {
  return useQuery(invitationsMineQuery())
}

export function useAcceptInvitation() {
  const invalidate = useInvalidate()
  return useMutation({
    ...invitationsAcceptMutation(),
    onSettled: () => invalidate('meGet', 'invitationsMine'),
  })
}

export function useDeclineInvitation() {
  const invalidate = useInvalidate()
  return useMutation({
    ...invitationsDeclineMutation(),
    onSettled: () => invalidate('invitationsMine'),
  })
}
