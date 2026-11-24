import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

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

export function useSigningRequest(
  workspaceId: MaybeRefOrGetter<string>,
  requestId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    signingRequestsGetQuery({
      path: { workspace_id: toValue(workspaceId), request_id: toValue(requestId) },
    }),
  )
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
