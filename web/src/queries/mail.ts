import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  mailSettingsDeleteMutation,
  mailSettingsGetQuery,
  mailSettingsPutMutation,
  mailSettingsTestMutation,
} from '@/api/gen/@pinia/colada.gen'
import type {
  AccountProvider,
  MailgunRegion,
  MailSettings,
  MailTestResult,
  SmtpSecurity,
  WorkspaceMail,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type {
  AccountProvider,
  MailgunRegion,
  MailSettings,
  MailTestResult,
  SmtpSecurity,
  WorkspaceMail,
}

export function useMailSettings(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => mailSettingsGetQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useSaveMailSettings() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailSettingsPutMutation(),
    // A refused save changed nothing, and a refetch would hand the form a
    // new object while the admin still has edits in it.
    onSuccess: () => invalidate('mailSettingsGet'),
  })
}

export function useDeleteMailSettings() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailSettingsDeleteMutation(),
    onSettled: () => invalidate('mailSettingsGet'),
  })
}

export function useTestMailSettings() {
  const invalidate = useInvalidate()
  return useMutation({
    ...mailSettingsTestMutation(),
    onSettled: () => invalidate('mailSettingsGet'),
  })
}
