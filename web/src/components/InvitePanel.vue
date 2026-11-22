<script setup lang="ts">
import { ref, watch } from 'vue'

import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
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
  <FormPanel
    :open="open"
    title="Invite someone"
    description="They get an email with a link. They sign in with that address to join."
    submit-label="Send invitation"
    :pending="invite.asyncStatus.value === 'loading'"
    :error="invite.error.value"
    @close="emit('close')"
    @submit="onInvite"
  >
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
  </FormPanel>
</template>
