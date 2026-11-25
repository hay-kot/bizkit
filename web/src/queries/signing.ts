import { useMutation, useQuery } from '@pinia/colada'
import { useIntervalFn } from '@vueuse/core'
import type { MaybeRefOrGetter } from 'vue'
import { toValue, watch } from 'vue'

import {
  signingPublicSessionQueryKey,
  signingPublicSignMutation,
  signingPublicViewedMutation,
} from '@/api/gen/@pinia/colada.gen'
import { signingPublicSession } from '@/api/gen/sdk.gen'
import type {
  RequestStatus,
  SigningField,
  SigningFieldFill,
  SigningFieldKind,
  SigningFieldValue,
  SigningSignature,
  SigningSignerSession,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type {
  RequestStatus,
  SigningField,
  SigningFieldFill,
  SigningFieldKind,
  SigningFieldValue,
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

/** How often the done view checks on the seal. */
const SEALING_POLL_MS = 2_000

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
 * What a signer's link shows. Polls while the signer has signed and the
 * seal hasn't finished, so the download appears on its own.
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
  // A refetch cancels one in flight, so the poll skips a tick while loading.
  const poll = useIntervalFn(
    () => {
      if (query.asyncStatus.value !== 'loading') void query.refetch()
    },
    SEALING_POLL_MS,
    { immediate: false },
  )
  watch(
    () => isSealing(query.data.value),
    (sealing) => (sealing ? poll.resume() : poll.pause()),
    { immediate: true },
  )
  return query
}

/** True once the signer signed and the sealed PDF isn't ready yet. */
export function isSealing(session: SigningSignerSession | undefined): boolean {
  return session?.signer.status === 'signed' && session.request.status === 'sent'
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
