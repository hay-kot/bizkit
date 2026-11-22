import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  formsCreateMutation,
  formsDeleteMutation,
  formsGetQuery,
  formsListQuery,
  formsSubmissionsDeleteMutation,
  formsSubmissionsListQuery,
  formsSubmissionsUpdateMutation,
  formsUpdateMutation,
} from '@/api/gen/@pinia/colada.gen'
import { formsSubmissionsExport } from '@/api/gen/sdk.gen'
import type {
  BotCheck,
  FieldDef,
  FieldKind,
  FieldValue,
  Form,
  Submission,
  SubmissionStatus,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { BotCheck, FieldDef, FieldKind, FieldValue, Form, Submission, SubmissionStatus }

const FORM_QUERIES = ['formsList', 'formsGet']
const SUBMISSION_QUERIES = ['formsSubmissionsList', 'formsGet', 'formsList']

export function useForms(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => formsListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useForm(workspaceId: MaybeRefOrGetter<string>, formId: MaybeRefOrGetter<string>) {
  return useQuery(() =>
    formsGetQuery({ path: { workspace_id: toValue(workspaceId), form_id: toValue(formId) } }),
  )
}

export function useCreateForm() {
  const invalidate = useInvalidate()
  return useMutation({ ...formsCreateMutation(), onSettled: () => invalidate(...FORM_QUERIES) })
}

export function useUpdateForm() {
  const invalidate = useInvalidate()
  return useMutation({ ...formsUpdateMutation(), onSettled: () => invalidate(...FORM_QUERIES) })
}

export function useDeleteForm() {
  const invalidate = useInvalidate()
  return useMutation({ ...formsDeleteMutation(), onSettled: () => invalidate(...FORM_QUERIES) })
}

export function useSubmissions(
  workspaceId: MaybeRefOrGetter<string>,
  formId: MaybeRefOrGetter<string>,
  status: MaybeRefOrGetter<SubmissionStatus>,
) {
  return useQuery(() =>
    formsSubmissionsListQuery({
      path: { workspace_id: toValue(workspaceId), form_id: toValue(formId) },
      query: { status: toValue(status), limit: 200 },
    }),
  )
}

export function useUpdateSubmission() {
  const invalidate = useInvalidate()
  return useMutation({
    ...formsSubmissionsUpdateMutation(),
    onSettled: () => invalidate(...SUBMISSION_QUERIES),
  })
}

export function useDeleteSubmission() {
  const invalidate = useInvalidate()
  return useMutation({
    ...formsSubmissionsDeleteMutation(),
    onSettled: () => invalidate(...SUBMISSION_QUERIES),
  })
}

/** Fetches the CSV export and hands it to the browser as a download. */
export async function downloadExport(
  workspaceId: string,
  formId: string,
  status: SubmissionStatus,
): Promise<{ filename: string; truncated: boolean }> {
  const { data } = await formsSubmissionsExport({
    path: { workspace_id: workspaceId, form_id: formId },
    query: { status },
    throwOnError: true,
  })
  const url = URL.createObjectURL(new Blob([data.csv], { type: 'text/csv' }))
  const link = document.createElement('a')
  link.href = url
  link.download = data.filename
  link.click()
  URL.revokeObjectURL(url)
  return { filename: data.filename, truncated: data.truncated }
}
