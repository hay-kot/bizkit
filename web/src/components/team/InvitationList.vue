<script setup lang="ts">
import { Mail, X } from 'lucide-vue-next'

import Avatar from '@/components/ui/Avatar.vue'
import List from '@/components/ui/List.vue'
import ListItem from '@/components/ui/ListItem.vue'
import { runAction } from '@/lib/action'
import { formatShortDate, isPast } from '@/lib/format'
import type { Invitation } from '@/queries/members'
import { useInvitations, useRevokeInvitation } from '@/queries/members'

const props = defineProps<{ workspaceId: string }>()
defineEmits<{ invite: [] }>()
const { data: invitations, isPending, error } = useInvitations(() => props.workspaceId)
const revoke = useRevokeInvitation()

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
  <List
    :items="invitations?.items"
    :item-key="(i) => i.id"
    :loading="isPending"
    :error="error"
    label="Pending invitations"
    :empty="{
      icon: Mail,
      title: 'No pending invitations',
      message: 'Invite teammates or outside collaborators by email.',
    }"
  >
    <template #item="{ item }">
      <ListItem :title="item.email">
        <template #leading><Avatar :name="item.email" /></template>
        <template #description
          ><span class="capitalize">{{ item.role }}</span></template
        >
        <template #meta>
          <span v-if="isPast(item.expires_at)" class="badge badge-ghost badge-sm">expired</span>
          <span v-else>Expires {{ formatShortDate(item.expires_at) }}</span>
        </template>
        <template #trailing>
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="`Revoke the invitation for ${item.email}`"
            @click="onRevoke(item)"
          >
            <X class="size-4" aria-hidden="true" />
          </button>
        </template>
      </ListItem>
    </template>
    <template #empty-action>
      <button type="button" class="btn btn-primary btn-sm" @click="$emit('invite')">
        Invite someone
      </button>
    </template>
  </List>
</template>
