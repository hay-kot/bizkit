<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import { useMe } from '@/queries/auth'
import { useAcceptInvitation, useDeclineInvitation, useMyInvitations } from '@/queries/workspaces'

const props = defineProps<{ id: string }>()
const router = useRouter()
const { data: me } = useMe()
const { data: invitations, isPending } = useMyInvitations()
const invitation = computed(() => invitations.value?.items.find((item) => item.id === props.id))
const accept = useAcceptInvitation()
const decline = useDeclineInvitation()

async function onAccept() {
  const membership = await accept.mutateAsync({ path: { invitation_id: props.id } })
  await router.replace(`/w/${membership.workspace.id}/projects`)
}

async function onDecline() {
  await decline.mutateAsync({ path: { invitation_id: props.id } })
  await router.replace('/')
}
</script>

<template>
  <Loading v-if="isPending" />
  <div v-else class="card mx-auto mt-12 max-w-md bg-base-100">
    <div v-if="invitation" class="card-body gap-4">
      <h1 class="card-title text-2xl">Join {{ invitation.workspace.name }}</h1>
      <p class="text-sm">
        <template v-if="invitation.invited_by">{{ invitation.invited_by }} invited you</template>
        <template v-else>You're invited</template>
        to join as {{ invitation.role === 'admin' ? 'an admin' : 'a member' }}.
      </p>
      <ErrorAlert :error="accept.error.value ?? decline.error.value" />
      <div class="card-actions justify-end">
        <button type="button" class="btn btn-ghost" @click="onDecline">Decline</button>
        <button type="button" class="btn btn-primary" @click="onAccept">Accept invitation</button>
      </div>
    </div>
    <div v-else class="card-body gap-4">
      <h1 class="card-title">Invitation not found</h1>
      <p class="text-sm">
        It may be for a different email than {{ me?.email ?? 'yours' }}, or it was revoked or has
        expired.
      </p>
      <RouterLink to="/" class="btn self-start btn-primary btn-sm"
        >Go to your workspaces</RouterLink
      >
    </div>
  </div>
</template>
