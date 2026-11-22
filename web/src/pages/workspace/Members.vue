<script setup lang="ts">
import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import type { Role } from '@/queries/members'
import { useMembers, useRemoveMember, useUpdateMember } from '@/queries/members'

const props = defineProps<{ workspaceId: string; isAdmin: boolean }>()
const { data: members, isPending, error } = useMembers(() => props.workspaceId)
const update = useUpdateMember()
const remove = useRemoveMember()

function setRole(userId: string, role: Role) {
  update.mutate({ path: { workspace_id: props.workspaceId, user_id: userId }, body: { role } })
}
</script>

<template>
  <section class="card bg-base-100" aria-labelledby="members-heading">
    <div class="card-body">
      <h2 id="members-heading" class="card-title text-lg">Members</h2>
      <ErrorAlert :error="update.error.value ?? remove.error.value" />
      <Loading v-if="isPending" />
      <ErrorAlert v-else-if="error" :error="error" />
      <div v-else-if="members" class="overflow-x-auto">
        <table class="table" data-testid="members">
          <thead>
            <tr>
              <th scope="col">Email</th>
              <th scope="col">Role</th>
              <th v-if="isAdmin" scope="col"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="member in members.items" :key="member.user_id">
              <td>{{ member.email }}</td>
              <td>
                <select
                  v-if="isAdmin"
                  class="select select-sm"
                  :value="member.role"
                  :aria-label="`Role of ${member.email}`"
                  @change="
                    setRole(member.user_id, ($event.target as HTMLSelectElement).value as Role)
                  "
                >
                  <option value="admin">admin</option>
                  <option value="member">member</option>
                </select>
                <span v-else class="badge badge-ghost">{{ member.role }}</span>
              </td>
              <td v-if="isAdmin" class="text-right">
                <button
                  type="button"
                  class="btn btn-ghost btn-xs"
                  :aria-label="`Remove ${member.email}`"
                  @click="
                    remove.mutate({ path: { workspace_id: workspaceId, user_id: member.user_id } })
                  "
                >
                  Remove
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </section>
</template>
