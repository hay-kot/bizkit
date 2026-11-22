<script setup lang="ts">
import { ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Field from '@/components/ui/Field.vue'
import Overlay from '@/components/ui/Overlay.vue'
import Select from '@/components/ui/Select.vue'
import { toast } from '@/lib/toast'
import type { Role } from '@/queries/members'
import { useInvite } from '@/queries/members'

const props = defineProps<{ workspaceId: string; open: boolean }>()
const emit = defineEmits<{ close: [] }>()
const invite = useInvite()
const email = ref('')
const role = ref<Role>('member')
const roles = [
  {
    value: 'member' as const,
    label: 'Member',
    description: 'Can read and change projects and data.',
  },
  {
    value: 'admin' as const,
    label: 'Admin',
    description: 'Can also manage members, invitations, keys and settings.',
  },
]

watch(
  () => props.open,
  (open) => {
    if (open) {
      email.value = ''
      role.value = 'member'
      invite.reset()
    }
  },
)

async function onInvite() {
  await invite.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { email: email.value, role: role.value },
  })
  toast(`Invitation sent to ${email.value}`)
  emit('close')
}
</script>

<template>
  <Overlay
    :open="open"
    variant="panel"
    title="Invite someone"
    description="They get an email with a link. They sign in with that address to join."
    @close="emit('close')"
  >
    <form id="invite-form" class="flex flex-col gap-4" @submit.prevent="onInvite">
      <Field label="Email">
        <input
          v-model="email"
          type="email"
          name="invite-email"
          required
          class="input w-full"
          placeholder="name@example.com"
        />
      </Field>
      <Field label="Role">
        <Select v-model="role" :options="roles" label="Role" name="invite-role" />
      </Field>
      <ErrorAlert :error="invite.error.value" />
    </form>
    <template #footer>
      <button type="button" class="btn btn-ghost btn-sm" @click="emit('close')">Cancel</button>
      <button
        type="submit"
        form="invite-form"
        class="btn btn-primary btn-sm"
        :disabled="invite.asyncStatus.value === 'loading'"
      >
        <span
          v-if="invite.asyncStatus.value === 'loading'"
          class="loading loading-xs loading-spinner"
        ></span>
        Send invitation
      </button>
    </template>
  </Overlay>
</template>
