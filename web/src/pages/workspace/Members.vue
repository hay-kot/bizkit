<script setup lang="ts">
import { UserPlus, UserX } from 'lucide-vue-next'
import { ref } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import InvitePanel from '@/components/InvitePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import Avatar from '@/components/ui/Avatar.vue'
import Select from '@/components/ui/Select.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { confirmAction } from '@/lib/confirm'
import { errorMessage } from '@/lib/errors'
import { toast } from '@/lib/toast'
import { useMe } from '@/queries/auth'
import type { Member, Role } from '@/queries/members'
import { useMembers, useRemoveMember, useUpdateMember } from '@/queries/members'

const props = defineProps<{ workspaceId: string; workspaceName: string; isAdmin: boolean }>()
const { data: members, isPending, error } = useMembers(() => props.workspaceId)
const { data: me } = useMe()
const update = useUpdateMember()
const remove = useRemoveMember()
const inviting = ref(false)
const roles = [
  { value: 'admin' as const, label: 'Admin' },
  { value: 'member' as const, label: 'Member' },
]

async function setRole(member: Member, role: Role) {
  if (role === member.role) return
  try {
    await update.mutateAsync({
      path: { workspace_id: props.workspaceId, user_id: member.user_id },
      body: { role },
    })
    toast(`${member.email} is now ${role === 'admin' ? 'an admin' : 'a member'}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}

async function onRemove(member: Member) {
  const self = member.user_id === me.value?.id
  const ok = await confirmAction({
    title: self ? 'Leave this workspace?' : `Remove ${member.email}?`,
    message: self
      ? 'You lose access right away. An admin has to invite you back.'
      : 'They lose access right away. You can invite them again later.',
    confirmLabel: self ? 'Leave' : 'Remove',
  })
  if (!ok) return
  try {
    await remove.mutateAsync({ path: { workspace_id: props.workspaceId, user_id: member.user_id } })
    toast(self ? `You left ${props.workspaceName}` : `Removed ${member.email}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}
</script>

<template>
  <div>
    <PageHeader
      :section="workspaceName"
      title="Members"
      description="Everyone who can work in this workspace."
    >
      <button
        v-if="isAdmin"
        type="button"
        class="btn gap-1.5 btn-primary btn-sm"
        @click="inviting = true"
      >
        <UserPlus class="size-4" aria-hidden="true" />Invite
      </button>
    </PageHeader>

    <SkeletonRows v-if="isPending" />
    <ErrorAlert v-else-if="error" :error="error" />
    <div v-else-if="members" class="overflow-x-auto rounded-box border border-base-300">
      <table class="table" data-testid="members">
        <thead>
          <tr>
            <th scope="col">Member</th>
            <th scope="col" class="w-40">Role</th>
            <th v-if="isAdmin" scope="col" class="w-12"><span class="sr-only">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="member in members.items"
            :key="member.user_id"
            class="transition-colors hover:bg-base-200"
          >
            <td>
              <div class="flex items-center gap-3">
                <Avatar :name="member.email" size="sm" />
                <span class="truncate">{{ member.email }}</span>
                <span v-if="member.user_id === me?.id" class="badge badge-ghost badge-sm">you</span>
              </div>
            </td>
            <td>
              <Select
                v-if="isAdmin"
                :model-value="member.role"
                :options="roles"
                :label="`Role of ${member.email}`"
                size="sm"
                @update:model-value="setRole(member, $event)"
              />
              <span v-else class="text-sm capitalize">{{ member.role }}</span>
            </td>
            <td v-if="isAdmin" class="text-right">
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :aria-label="`Remove ${member.email}`"
                @click="onRemove(member)"
              >
                <UserX class="size-4" aria-hidden="true" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <InvitePanel :workspace-id="workspaceId" :open="inviting" @close="inviting = false" />
  </div>
</template>
