<script setup lang="ts">
import { UserX } from 'lucide-vue-next'

import Avatar from '@/components/ui/Avatar.vue'
import DataTable from '@/components/ui/DataTable.vue'
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
const columns = [
  { key: 'member', label: 'Member' },
  { key: 'role', label: 'Role', class: 'w-40' },
  ...(props.isAdmin
    ? [{ key: 'actions', label: 'Actions', hidden: true, class: 'w-12 text-right' }]
    : []),
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
  <div>
    <DataTable
      :columns="columns"
      :rows="members?.items"
      :row-key="(m) => m.user_id"
      :loading="isPending"
      :error="error"
      label="Members"
    >
      <template #cell-member="{ row }">
        <div class="flex items-center gap-3">
          <Avatar :name="row.email" size="sm" />
          <span class="truncate">{{ row.email }}</span>
          <span v-if="row.user_id === me?.id" class="badge badge-ghost badge-sm">you</span>
        </div>
      </template>
      <template #cell-role="{ row }">
        <Select
          v-if="isAdmin"
          :model-value="row.role"
          :options="roles"
          :label="`Role of ${row.email}`"
          size="sm"
          @update:model-value="setRole(row, $event)"
        />
        <span v-else class="text-sm capitalize">{{ row.role }}</span>
      </template>
      <template #cell-actions="{ row }">
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Remove ${row.email}`"
          @click="onRemove(row)"
        >
          <UserX class="size-4" aria-hidden="true" />
        </button>
      </template>
    </DataTable>
  </div>
</template>
