import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { onScopeDispose, ref, toValue, watch } from 'vue'

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
    () => isSealing(query.data.value),
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
export function isSealing(session: SigningSignerSession | undefined): boolean {
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
