import { useMutation, useQuery } from '@pinia/colada'
import { useIntervalFn } from '@vueuse/core'
import type { MaybeRefOrGetter } from 'vue'
import { toValue, watch } from 'vue'

import {
  mailingBroadcastsCreateMutation,
  mailingBroadcastsDeleteMutation,
  mailingBroadcastsGetQuery,
  mailingBroadcastsListQuery,
  mailingBroadcastsSendMutation,
  mailingBroadcastsTestMutation,
  mailingBroadcastsUpdateMutation,
  mailingContactsCreateMutation,
  mailingContactsDeleteMutation,
  mailingContactsListQuery,
  mailingContactsUnsubscribeMutation,
  mailingDeliveriesListQuery,
  mailingFormsListQuery,
  mailingListsCreateMutation,
  mailingListsDeleteMutation,
  mailingListsGetQuery,
  mailingListsListQuery,
  mailingListsUpdateMutation,
  mailingPreviewMutation,
} from '@/api/gen/@pinia/colada.gen'
import type {
  Broadcast,
  BroadcastStatus,
  Contact,
  ContactStatus,
  DeliveryStatus,
  MailDelivery,
  MailingList,
  Preview,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type {
  Broadcast,
  BroadcastStatus,
  Contact,
  ContactStatus,
  DeliveryStatus,
  MailDelivery,
  MailingList,
  Preview,
}

const LIST_QUERIES = ['mailingListsList', 'mailingListsGet']
const CONTACT_QUERIES = ['mailingContactsList', 'mailingListsGet']
const BROADCAST_QUERIES = ['mailingBroadcastsList', 'mailingBroadcastsGet']

/** The most contacts and deliveries a page lists. */
export const ROWS_SHOWN = 100

export function useMailingLists(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery(() => ({
    ...mailingListsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
    enabled: toValue(enabled),
  }))
}

/** One list with its counts; idle until `listId` is set. */
export function useMailingList(
  workspaceId: MaybeRefOrGetter<string>,
  listId: MaybeRefOrGetter<string | null>,
) {
  return useQuery(() => ({
    ...mailingListsGetQuery({
      path: { workspace_id: toValue(workspaceId), list_id: toValue(listId) ?? '' },
    }),
    enabled: Boolean(toValue(listId)),
  }))
}

/** The project's forms a list can take addresses from. */
export function useMailingForms(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    mailingFormsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useCreateMailingList() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingListsCreateMutation(),
    onSettled: () => invalidate(...LIST_QUERIES),
  })
}

export function useUpdateMailingList() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingListsUpdateMutation(),
    onSettled: () => invalidate(...LIST_QUERIES),
  })
}

export function useDeleteMailingList() {
  const invalidate = useInvalidate()
  // Only the lists: the deleted list's page is still mounted, and refetching
  // it would flash a 404 before the page leaves.
  return useMutation({
    ...mailingListsDeleteMutation(),
    onSettled: () => invalidate('mailingListsList', 'mailingBroadcastsList'),
  })
}

export function useContacts(
  workspaceId: MaybeRefOrGetter<string>,
  listId: MaybeRefOrGetter<string>,
  filter: MaybeRefOrGetter<{ status?: ContactStatus; q?: string }>,
) {
  return useQuery(() => {
    const { status, q } = toValue(filter)
    return mailingContactsListQuery({
      path: { workspace_id: toValue(workspaceId), list_id: toValue(listId) },
      query: { limit: ROWS_SHOWN, status, q: q?.trim() ? q.trim() : undefined },
    })
  })
}

export function useAddContact() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingContactsCreateMutation(),
    onSettled: () => invalidate(...CONTACT_QUERIES),
  })
}

export function useUnsubscribeContact() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingContactsUnsubscribeMutation(),
    onSettled: () => invalidate(...CONTACT_QUERIES),
  })
}

export function useDeleteContact() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingContactsDeleteMutation(),
    onSettled: () => invalidate(...CONTACT_QUERIES),
  })
}

export function useBroadcasts(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    mailingBroadcastsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

const SENDING_POLL_MS = 2_000

/** A broadcast; while it is sending, it refetches every two seconds. */
export function useBroadcast(
  workspaceId: MaybeRefOrGetter<string>,
  broadcastId: MaybeRefOrGetter<string>,
) {
  const query = useQuery(() =>
    mailingBroadcastsGetQuery({
      path: { workspace_id: toValue(workspaceId), broadcast_id: toValue(broadcastId) },
    }),
  )
  // A refetch cancels one in flight, and a mutation that awaits its
  // invalidation would then fail, so the poll skips a tick while loading.
  const poll = useIntervalFn(
    () => {
      if (query.asyncStatus.value !== 'loading') void query.refetch()
    },
    SENDING_POLL_MS,
    { immediate: false },
  )
  watch(
    () => query.data.value?.status === 'sending',
    (sending) => (sending ? poll.resume() : poll.pause()),
    { immediate: true },
  )
  return query
}

export function useCreateBroadcast() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingBroadcastsCreateMutation(),
    onSettled: () => invalidate(...BROADCAST_QUERIES),
  })
}

export function useUpdateBroadcast() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingBroadcastsUpdateMutation(),
    onSettled: () => invalidate(...BROADCAST_QUERIES),
  })
}

export function useDeleteBroadcast() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingBroadcastsDeleteMutation(),
    onSettled: () => invalidate('mailingBroadcastsList'),
  })
}

export function useSendBroadcast() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailingBroadcastsSendMutation(),
    onSettled: () => invalidate(...BROADCAST_QUERIES, 'mailingDeliveriesList'),
  })
}

export function useTestBroadcast() {
  return useMutation(mailingBroadcastsTestMutation())
}

export function usePreview() {
  return useMutation(mailingPreviewMutation())
}

/** A broadcast's send log. */
export function useDeliveries(
  workspaceId: MaybeRefOrGetter<string>,
  broadcastId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    mailingDeliveriesListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { broadcast_id: toValue(broadcastId), limit: ROWS_SHOWN },
    }),
  )
}
