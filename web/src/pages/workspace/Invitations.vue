<script setup lang="ts">
import { ref } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import type { Role } from '@/queries/members'
import { useInvitations, useInvite, useRevokeInvitation } from '@/queries/members'

const props = defineProps<{ workspaceId: string }>()
const { data: invitations, isPending, error } = useInvitations(() => props.workspaceId)
const invite = useInvite()
const revoke = useRevokeInvitation()
const email = ref('')
const role = ref<Role>('member')
const sentTo = ref('')

async function onInvite() {
  await invite.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { email: email.value, role: role.value },
  })
  sentTo.value = email.value
  email.value = ''
}

function formatDate(value: string) {
  return new Date(value).toLocaleDateString()
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <section class="card bg-base-100" aria-labelledby="invite-heading">
      <form class="card-body gap-4" @submit.prevent="onInvite">
        <h2 id="invite-heading" class="card-title text-lg">Invite someone</h2>
        <div class="grid gap-4 sm:grid-cols-3">
          <label class="flex flex-col gap-1 sm:col-span-2">
            <span class="text-sm font-medium">Email</span>
            <input v-model="email" type="email" name="invite-email" required class="input w-full" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium">Role</span>
            <select v-model="role" name="invite-role" class="select w-full">
              <option value="member">member</option>
              <option value="admin">admin</option>
            </select>
          </label>
        </div>
        <div v-if="sentTo" role="status" class="alert alert-success">
          <span>Invitation sent to {{ sentTo }}.</span>
        </div>
        <ErrorAlert :error="invite.error.value" />
        <div class="card-actions justify-end">
          <button type="submit" class="btn btn-primary">Send invitation</button>
        </div>
      </form>
    </section>
    <section class="card bg-base-100" aria-labelledby="pending-heading">
      <div class="card-body">
        <h2 id="pending-heading" class="card-title text-lg">Pending invitations</h2>
        <Loading v-if="isPending" />
        <ErrorAlert v-else-if="error" :error="error" />
        <EmptyState v-else-if="!invitations?.items.length" message="No pending invitations." />
        <div v-else class="overflow-x-auto">
          <table class="table">
            <thead>
              <tr>
                <th scope="col">Email</th>
                <th scope="col">Role</th>
                <th scope="col">Expires</th>
                <th scope="col"><span class="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in invitations.items" :key="item.id">
                <td>{{ item.email }}</td>
                <td>{{ item.role }}</td>
                <td>{{ formatDate(item.expires_at) }}</td>
                <td class="text-right">
                  <button
                    type="button"
                    class="btn btn-ghost btn-xs"
                    :aria-label="`Revoke the invitation for ${item.email}`"
                    @click="
                      revoke.mutate({ path: { workspace_id: workspaceId, invitation_id: item.id } })
                    "
                  >
                    Revoke
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>
  </div>
</template>
