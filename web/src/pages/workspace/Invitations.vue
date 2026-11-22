<script setup lang="ts">
import { Mail, MailPlus, X } from 'lucide-vue-next'
import { ref } from 'vue'

import InvitePanel from '@/components/InvitePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import DataTable from '@/components/ui/DataTable.vue'
import { runAction } from '@/lib/action'
import { formatShortDate, isPast } from '@/lib/format'
import type { Invitation } from '@/queries/members'
import { useInvitations, useRevokeInvitation } from '@/queries/members'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: invitations, isPending, error } = useInvitations(() => props.workspaceId)
const revoke = useRevokeInvitation()
const inviting = ref(false)
const columns = [
  { key: 'email', label: 'Email' },
  { key: 'role', label: 'Role' },
  { key: 'expires', label: 'Expires' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-12 text-right' },
]

function onRevoke(item: Invitation) {
  return runAction({
    confirm: {
      title: `Revoke the invitation for ${item.email}?`,
      message: 'The link in their email stops working.',
      confirmLabel: 'Revoke',
    },
    run: () =>
      revoke.mutateAsync({ path: { workspace_id: props.workspaceId, invitation_id: item.id } }),
    success: `Revoked the invitation for ${item.email}`,
  })
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

    <DataTable
      :columns="columns"
      :rows="invitations?.items"
      :row-key="(i) => i.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: Mail,
        title: 'No pending invitations',
        message: 'Invite teammates or outside collaborators by email.',
      }"
    >
      <template #cell-email="{ row }">{{ row.email }}</template>
      <template #cell-role="{ row }"
        ><span class="capitalize">{{ row.role }}</span></template
      >
      <template #cell-expires="{ row }">
        <span v-if="isPast(row.expires_at)" class="badge badge-ghost badge-sm">expired</span>
        <span v-else class="text-sm">{{ formatShortDate(row.expires_at) }}</span>
      </template>
      <template #cell-actions="{ row }">
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Revoke the invitation for ${row.email}`"
          @click="onRevoke(row)"
        >
          <X class="size-4" aria-hidden="true" />
        </button>
      </template>
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="inviting = true">
          Invite someone
        </button>
      </template>
    </DataTable>

    <InvitePanel :workspace-id="workspaceId" :open="inviting" @close="inviting = false" />
  </div>
</template>
