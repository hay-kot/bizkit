<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Avatar from '@/components/ui/Avatar.vue'
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
  await router.replace(`/w/${membership.workspace.id}`)
}

async function onDecline() {
  await decline.mutateAsync({ path: { invitation_id: props.id } })
  await router.replace('/')
}
</script>

<template>
  <div class="mx-auto mt-8 max-w-md">
    <div v-if="isPending" class="h-48 animate-pulse rounded-box bg-base-300"></div>
    <div
      v-else-if="invitation"
      class="flex flex-col items-center gap-4 rounded-box border border-base-300 bg-base-100 p-8 text-center"
    >
      <Avatar :name="invitation.workspace.name" size="lg" />
      <div>
        <h1 class="text-xl font-semibold">Join {{ invitation.workspace.name }}</h1>
        <p class="mt-1 text-sm text-base-content/80">
          <template v-if="invitation.invited_by">{{ invitation.invited_by }} invited you</template>
          <template v-else>You're invited</template>
          to join as {{ invitation.role === 'admin' ? 'an admin' : 'a member' }}.
        </p>
      </div>
      <ErrorAlert :error="accept.error.value ?? decline.error.value" />
      <div class="flex gap-2">
        <button type="button" class="btn btn-ghost" @click="onDecline">Decline</button>
        <button type="button" class="btn btn-primary" @click="onAccept">Accept invitation</button>
      </div>
    </div>
    <div v-else class="flex flex-col gap-3 rounded-box border border-base-300 bg-base-100 p-8">
      <h1 class="text-xl font-semibold">Invitation not found</h1>
      <p class="text-sm text-base-content/80">
        It may be for a different email than {{ me?.email ?? 'yours' }}, or it was revoked or has
        expired.
      </p>
      <RouterLink to="/" class="btn self-start btn-primary btn-sm"
        >Go to your workspaces</RouterLink
      >
    </div>
  </div>
</template>
