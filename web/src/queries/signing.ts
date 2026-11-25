import { useMutation, useQuery } from '@pinia/colada'
import { useIntervalFn } from '@vueuse/core'
import type { MaybeRefOrGetter } from 'vue'
import { onScopeDispose, ref, toValue, watch } from 'vue'

import {
  signingPublicSessionQueryKey,
  signingPublicSignMutation,
  signingPublicViewedMutation,
  signingRequestsDeleteMutation,
  signingRequestsGetQuery,
  signingRequestsListQuery,
  signingRequestsRetryMutation,
  signingRequestsSendMutation,
  signingRequestsVoidMutation,
} from '@/api/gen/@pinia/colada.gen'
import {
  signingPublicSession,
  signingRequestsFieldsPut,
  signingRequestsSignersPut,
  signingUploadCreate,
} from '@/api/gen/sdk.gen'
import type {
  AuditEvent,
  DocumentVersion,
  DraftMeta,
  EventKind,
  NewSigner,
  RequestStatus,
  Signer,
  SignerSlot,
  SigningField,
  SigningFieldFill,
  SigningFieldInput,
  SigningFieldKind,
  SigningFieldValue,
  SigningRequest,
  SigningRequestDetail,
  SigningSignature,
  SigningSignerSession,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type {
  AuditEvent,
  DocumentVersion,
  EventKind,
  NewSigner,
  RequestStatus,
  Signer,
  SignerSlot,
  SigningField,
  SigningFieldFill,
  SigningFieldInput,
  SigningFieldKind,
  SigningFieldValue,
  SigningRequest,
  SigningRequestDetail,
  SigningSignature,
  SigningSignerSession,
}

/** The pad's coordinate space, as the server checks it. */
export const PAD_WIDTH = 1000
export const PAD_HEIGHT = 300
/** The most strokes and points a drawn signature may have. */
export const MAX_STROKES = 50
export const MAX_POINTS = 2000
/** The longest typed signature and text fill. */
export const MAX_TYPED = 100
export const MAX_TEXT = 200

/** The done view's checks on the seal: 2 s, then slower, for at most 2 minutes. */
const SEAL_POLL_FIRST_MS = 2_000
const SEAL_POLL_MAX_MS = 15_000
const SEAL_POLL_FOR_MS = 120_000

/** The wait before the next check on the seal, or `undefined` once it's time to stop. */
export function nextSealPoll(attempt: number, waited: number): number | undefined {
  if (waited >= SEAL_POLL_FOR_MS) return undefined
  return Math.min(SEAL_POLL_FIRST_MS * 1.5 ** attempt, SEAL_POLL_MAX_MS)
}

const NOT_FOUND = { error: { code: 'not_found', message: 'This link no longer works.' } }

/** Where a signer downloads the sealed PDF once the request completes. */
export function signedPdfUrl(token: string): string {
  return `/m/signing/public/signed.pdf?t=${encodeURIComponent(token)}`
}

/** Where pdf.js reads the PDF to sign. */
export function documentUrl(token: string): string {
  return `/m/signing/public/document?t=${encodeURIComponent(token)}`
}

/**
 * What a signer's link shows. Polls while everyone has signed and the seal
 * runs, so the download appears on its own; `pollGaveUp` turns true when
 * the seal took longer than the polling lasts.
 */
export function useSignerSession(token: MaybeRefOrGetter<string>) {
  const query = useQuery<SigningSignerSession>(() => ({
    key: signingPublicSessionQueryKey({ query: { t: toValue(token) } }),
    enabled: toValue(token) !== '',
    query: async ({ signal }) => {
      const { data } = await signingPublicSession({
        query: { t: toValue(token) },
        signal,
        throwOnError: true,
      })
      // A build without signing serves the web app for unknown /m/ paths,
      // so the answer is HTML rather than a 404.
      if (typeof data !== 'object' || data === null || !('request' in data)) throw NOT_FOUND
      return data
    },
  }))
  // Polls only while the seal job is what's pending: waiting on other
  // signers can take days, and their signing doesn't need this page open.
  const pollGaveUp = ref(false)
  let timer: ReturnType<typeof setTimeout> | undefined
  const stop = () => {
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
  }
  watch(
    () => isSignerSealing(query.data.value),
    (sealing) => {
      stop()
      pollGaveUp.value = false
      if (!sealing) return
      let attempt = 0
      let waited = 0
      const schedule = () => {
        const wait = nextSealPoll(attempt, waited)
        if (wait === undefined) {
          pollGaveUp.value = true
          return
        }
        timer = setTimeout(() => {
          attempt++
          waited += wait
          // A refetch cancels one in flight, so a tick skips while loading.
          if (query.asyncStatus.value !== 'loading') void query.refetch()
          schedule()
        }, wait)
      }
      schedule()
    },
    { immediate: true },
  )
  onScopeDispose(stop)
  return { ...query, pollGaveUp }
}

/** True once everyone signed and the seal job hasn't finished. */
export function isSignerSealing(session: SigningSignerSession | undefined): boolean {
  return session?.all_signed === true && session.request.status === 'sent'
}

export function useRecordView() {
  return useMutation(signingPublicViewedMutation())
}

export function useSign() {
  const invalidate = useInvalidate()
  return useMutation({
    ...signingPublicSignMutation(),
    onSuccess: () => invalidate('signingPublicSession'),
  })
}

// ---------------------------------------------------------------- sender

const REQUEST_QUERIES = ['signingRequestsList', 'signingRequestsGet']

/** The largest PDF the upload route takes, in bytes. */
export const MAX_UPLOAD_BYTES = 25 * 1024 * 1024

/** The most signers one request takes. */
export const MAX_SIGNERS = 10

/** Signing-order groups run from 1 to 10. */
export const MAX_ORDER = 10

/** The most fields one request takes. */
export const MAX_FIELDS = 200

/** The longest field label. */
export const MAX_LABEL = 100

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

/** One request with its fields and audit trail; polls while its seal runs. */
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

const TOO_LARGE = { error: { code: 'too_large', message: 'The PDF must be at most 25 MiB.' } }
const TIMED_OUT = {
  error: {
    code: 'timeout',
    message: 'The upload took too long. Try again on a faster connection.',
  },
}

/**
 * Uploads `file` and creates the draft through the multipart route. The
 * body limit answers 413 in plain text before bizkit reads the body, so
 * 413 and 408 get their own messages; a 422 keeps its field errors.
 */
async function uploadDraft(workspaceId: string, meta: DraftMeta, file: File) {
  const { data, error, response } = await signingUploadCreate({
    path: { workspace_id: workspaceId },
    body: { meta, file },
  })
  if (data) return data
  if (response?.status === 413) throw TOO_LARGE
  if (response?.status === 408) throw TIMED_OUT
  throw error ?? new Error('The upload failed. Try again.')
}

export function useUploadSigningRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    mutation: (vars: { workspaceId: string; meta: DraftMeta; file: File }) =>
      uploadDraft(vars.workspaceId, vars.meta, vars.file),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}

/** A draft's whole signer list, its whole field list, or both, to save. */
export interface DraftSave {
  workspaceId: string
  requestId: string
  /** Only when the signers changed. */
  signers?: SignerSlot[]
  /**
   * The fields, built once the signers have IDs: `signerIds` holds the
   * saved signers' IDs in the order of `signers`.
   */
  fields: (signerIds: string[]) => SigningFieldInput[]
  /** Called once the signers saved, before the fields go. */
  onSigners?: (detail: SigningRequestDetail) => void
}

/**
 * Saves a draft: `signers.put`, then `fields.put`, since new signers need
 * their IDs before fields can name them. Gives the final detail.
 */
export function useSaveSigningDraft() {
  const invalidate = useInvalidate()
  return useMutation({
    mutation: async (save: DraftSave) => {
      const path = { workspace_id: save.workspaceId, request_id: save.requestId }
      let ids: string[] = []
      if (save.signers) {
        const { data } = await signingRequestsSignersPut({
          path,
          body: { signers: save.signers },
          throwOnError: true,
        })
        save.onSigners?.(data)
        ids = data.request.signers.map((signer) => signer.id)
      }
      const { data } = await signingRequestsFieldsPut({
        path,
        body: { fields: save.fields(ids) },
        throwOnError: true,
      })
      return data
    },
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

/**
 * Where the web reads a request's original or sealed PDF. These file
 * routes stream the bytes, so a 25 MiB file never goes through base64 JSON.
 */
export function requestFileUrl(
  workspaceId: string,
  requestId: string,
  version: DocumentVersion,
): string {
  const ws = encodeURIComponent(workspaceId)
  const id = encodeURIComponent(requestId)
  return `/m/signing/workspaces/${ws}/requests/${id}/${version}.pdf`
}
