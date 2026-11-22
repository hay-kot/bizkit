import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  invitationsCreateMutation,
  invitationsListQuery,
  invitationsRevokeMutation,
  membersListQuery,
  membersRemoveMutation,
  membersUpdateMutation,
} from '@/api/gen/@pinia/colada.gen'
import type { Invitation, Member, Role } from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { Invitation, Member, Role }

export function useMembers(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => membersListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useUpdateMember() {
  const invalidate = useInvalidate()
  return useMutation({
    ...membersUpdateMutation(),
    onSettled: () => invalidate('membersList', 'meGet'),
  })
}

export function useRemoveMember() {
  const invalidate = useInvalidate()
  return useMutation({
    ...membersRemoveMutation(),
    onSettled: () => invalidate('membersList', 'meGet'),
  })
}

export function useInvitations(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => invitationsListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useInvite() {
  const invalidate = useInvalidate()
  return useMutation({
    ...invitationsCreateMutation(),
    onSettled: () => invalidate('invitationsList'),
  })
}

export function useRevokeInvitation() {
  const invalidate = useInvalidate()
  return useMutation({
    ...invitationsRevokeMutation(),
    onSettled: () => invalidate('invitationsList'),
  })
}
