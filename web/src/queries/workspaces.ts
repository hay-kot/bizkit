import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  invitationsAcceptMutation,
  invitationsDeclineMutation,
  invitationsMineQuery,
  workspacesCreateMutation,
  workspacesGetQuery,
  workspacesUpdateMutation,
} from '@/api/gen/@pinia/colada.gen'
import type { Membership, MyInvitation, Workspace } from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { Membership, MyInvitation, Workspace }

export function useWorkspace(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => workspacesGetQuery({ path: { workspace_id: toValue(workspaceId) } }))
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
