import { useMutation, useQuery } from '@pinia/colada'
import { useIntervalFn } from '@vueuse/core'
import type { MaybeRefOrGetter } from 'vue'
import { toValue, watch } from 'vue'

import {
  signingRequestsCreateMutation,
  signingRequestsDeleteMutation,
  signingRequestsGetQuery,
  signingRequestsListQuery,
  signingRequestsRetryMutation,
  signingRequestsSendMutation,
  signingRequestsVoidMutation,
} from '@/api/gen/@pinia/colada.gen'
import { signingRequestsDocument } from '@/api/gen/sdk.gen'
import type {
  AuditEvent,
  DocumentVersion,
  EventKind,
  NewSigner,
  RequestStatus,
  Signer,
  SigningRequest,
} from '@/api/gen/types.gen'
import { saveBase64 } from '@/lib/files'

import { useInvalidate } from './keys'

export type {
  AuditEvent,
  DocumentVersion,
  EventKind,
  NewSigner,
  RequestStatus,
  Signer,
  SigningRequest,
}

const REQUEST_QUERIES = ['signingRequestsList', 'signingRequestsGet']

/** The largest PDF the API takes, in bytes. */
export const MAX_PDF_BYTES = 700 * 1024

/** The most signers one request takes. */
export const MAX_SIGNERS = 10

export function useSigningRequests(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    signingRequestsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

/** How often a request's page checks on a seal in progress. */
const SEALING_POLL_MS = 2_000

/** True while every signer signed and the seal hasn't finished. */
export function isSealing(request: SigningRequest | undefined): boolean {
  return request?.status === 'sent' && request.signers.every((s) => s.signed)
}

/** One request with its audit trail; polls while its seal runs. */
export function useSigningRequest(
  workspaceId: MaybeRefOrGetter<string>,
  requestId: MaybeRefOrGetter<string>,
) {
  const query = useQuery(() =>
    signingRequestsGetQuery({
      path: { workspace_id: toValue(workspaceId), request_id: toValue(requestId) },
    }),
  )
  // A refetch cancels one in flight, and a mutation that awaits its
  // invalidation would then fail, so the poll skips a tick while loading.
  const poll = useIntervalFn(
    () => {
      if (query.asyncStatus.value !== 'loading') void query.refetch()
    },
    SEALING_POLL_MS,
    { immediate: false },
  )
  watch(
    () => isSealing(query.data.value?.request),
    (sealing) => (sealing ? poll.resume() : poll.pause()),
    { immediate: true },
  )
  return query
}

export function useCreateSigningRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    ...signingRequestsCreateMutation(),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}

export function useSendSigningRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    ...signingRequestsSendMutation(),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}

export function useVoidSigningRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    ...signingRequestsVoidMutation(),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}

export function useRetrySigningRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    ...signingRequestsRetryMutation(),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}

export function useDeleteSigningRequest() {
  const invalidate = useInvalidate()
  // Only the list: the deleted request's page is still mounted, and
  // refetching it would flash a 404 before the page leaves.
  return useMutation({
    ...signingRequestsDeleteMutation(),
    onSettled: () => invalidate('signingRequestsList'),
  })
}

/** Fetches a request's original or sealed PDF and hands it to the browser as a download. */
export async function downloadSigningDocument(
  workspaceId: string,
  requestId: string,
  version: DocumentVersion,
): Promise<void> {
  const { data } = await signingRequestsDocument({
    path: { workspace_id: workspaceId, request_id: requestId },
    query: { version },
    throwOnError: true,
  })
  saveBase64(data.pdf_base64, data.file_name, 'application/pdf')
}
