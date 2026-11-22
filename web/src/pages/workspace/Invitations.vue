<script setup lang="ts">
import { Mail, MailPlus, X } from 'lucide-vue-next'
import { ref } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import InvitePanel from '@/components/InvitePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { confirmAction } from '@/lib/confirm'
import { errorMessage } from '@/lib/errors'
import { toast } from '@/lib/toast'
import type { Invitation } from '@/queries/members'
import { useInvitations, useRevokeInvitation } from '@/queries/members'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: invitations, isPending, error } = useInvitations(() => props.workspaceId)
const revoke = useRevokeInvitation()
const inviting = ref(false)

function isExpired(item: Invitation) {
  return new Date(item.expires_at).getTime() < Date.now()
}

function formatDate(value: string) {
  return new Date(value).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

async function onRevoke(item: Invitation) {
  const ok = await confirmAction({
    title: `Revoke the invitation for ${item.email}?`,
    message: 'The link in their email stops working.',
    confirmLabel: 'Revoke',
  })
  if (!ok) return
  try {
    await revoke.mutateAsync({ path: { workspace_id: props.workspaceId, invitation_id: item.id } })
    toast(`Revoked the invitation for ${item.email}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}
</script>

<template>
  <div>
    <PageHeader
      :section="workspaceName"
      title="Invitations"
      description="Invitations last 7 days. Revoke one to cancel it."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="inviting = true">
        <MailPlus class="size-4" aria-hidden="true" />Invite
      </button>
    </PageHeader>

    <SkeletonRows v-if="isPending" />
    <ErrorAlert v-else-if="error" :error="error" />
    <EmptyState
      v-else-if="!invitations?.items.length"
      :icon="Mail"
      title="No pending invitations"
      message="Invite teammates or outside collaborators by email."
    >
      <button type="button" class="btn btn-primary btn-sm" @click="inviting = true">
        Invite someone
      </button>
    </EmptyState>
    <div v-else class="overflow-x-auto rounded-box border border-base-300">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">Email</th>
            <th scope="col">Role</th>
            <th scope="col">Expires</th>
            <th scope="col" class="w-12"><span class="sr-only">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="item in invitations.items"
            :key="item.id"
            class="transition-colors hover:bg-base-200"
          >
            <td>{{ item.email }}</td>
            <td class="capitalize">{{ item.role }}</td>
            <td>
              <span v-if="isExpired(item)" class="badge badge-ghost badge-sm">expired</span>
              <span v-else class="text-sm">{{ formatDate(item.expires_at) }}</span>
            </td>
            <td class="text-right">
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :aria-label="`Revoke the invitation for ${item.email}`"
                @click="onRevoke(item)"
              >
                <X class="size-4" aria-hidden="true" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <InvitePanel :workspace-id="workspaceId" :open="inviting" @close="inviting = false" />
  </div>
</template>
