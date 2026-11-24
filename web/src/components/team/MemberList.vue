<script setup lang="ts">
import { UserX } from 'lucide-vue-next'

import Avatar from '@/components/ui/Avatar.vue'
import List from '@/components/ui/List.vue'
import ListItem from '@/components/ui/ListItem.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { useMe } from '@/queries/auth'
import type { Member, Role } from '@/queries/members'
import { useMembers, useRemoveMember, useUpdateMember } from '@/queries/members'

const props = defineProps<{ workspaceId: string; workspaceName: string; isAdmin: boolean }>()
const { data: members, isPending, error } = useMembers(() => props.workspaceId)
const { data: me } = useMe()
const update = useUpdateMember()
const remove = useRemoveMember()
const roles = [
  { value: 'admin' as const, label: 'Admin' },
  { value: 'member' as const, label: 'Member' },
]

function setRole(member: Member, role: Role) {
  if (role === member.role) return
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, user_id: member.user_id },
        body: { role },
      }),
    success: `${member.email} is now ${role === 'admin' ? 'an admin' : 'a member'}`,
  })
}

function onRemove(member: Member) {
  const self = member.user_id === me.value?.id
  return runAction({
    confirm: {
      title: self ? 'Leave this workspace?' : `Remove ${member.email}?`,
      message: self
        ? 'You lose access right away. An admin has to invite you back.'
        : 'They lose access right away. You can invite them again later.',
      confirmLabel: self ? 'Leave' : 'Remove',
    },
    run: () =>
      remove.mutateAsync({ path: { workspace_id: props.workspaceId, user_id: member.user_id } }),
    success: self ? `You left ${props.workspaceName}` : `Removed ${member.email}`,
  })
}
</script>

<template>
  <List
    :items="members?.items"
    :item-key="(m) => m.user_id"
    :loading="isPending"
    :error="error"
    label="Members"
  >
    <template #item="{ item }">
      <ListItem>
        <template #leading><Avatar :name="item.email" /></template>
        <template #title
          >{{ item.email }}
          <span v-if="item.user_id === me?.id" class="ms-1 badge badge-ghost badge-sm"
            >you</span
          ></template
        >
        <template v-if="!isAdmin" #meta>
          <span class="capitalize">{{ item.role }}</span>
        </template>
        <template v-if="isAdmin" #trailing>
          <div class="w-32">
            <Select
              :model-value="item.role"
              :options="roles"
              :label="`Role of ${item.email}`"
              size="sm"
              @update:model-value="setRole(item, $event)"
            />
          </div>
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="`Remove ${item.email}`"
            @click="onRemove(item)"
          >
            <UserX class="size-4" aria-hidden="true" />
          </button>
        </template>
      </ListItem>
    </template>
  </List>
</template>
