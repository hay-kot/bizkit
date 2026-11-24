<script setup lang="ts">
import { ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Card from '@/components/ui/Card.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import Field from '@/components/ui/Field.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { toast } from '@/lib/toast'
import { useRenameWorkspace, useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: workspace } = useWorkspace(() => props.workspaceId)
const rename = useRenameWorkspace()
const name = ref('')
watch(workspace, (value) => (name.value = value?.name ?? ''), { immediate: true })

async function onSave() {
  await rename.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value },
  })
  toast('Workspace renamed')
}
</script>

<template>
  <Page>
    <PageHeader title="Settings" />
    <form @submit.prevent="onSave">
      <Card title="General" description="The name shows in the switcher and in invitation emails.">
        <Field label="Name" width="md">
          <input v-model="name" name="workspace-name" required class="input w-full" />
        </Field>
        <Field
          label="Slug"
          width="md"
          hint="The slug never changes: email policies join workspaces by slug."
        >
          <div class="flex items-center gap-2">
            <input
              :value="workspace?.slug"
              readonly
              class="input w-full bg-base-300/50"
              aria-readonly="true"
            />
            <CopyButton v-if="workspace" :text="workspace.slug" label="Copy slug" />
          </div>
        </Field>
        <Field label="Workspace ID" width="md">
          <div class="flex items-center gap-2">
            <input
              :value="workspaceId"
              readonly
              class="input w-full bg-base-300/50 font-mono text-sm"
              aria-readonly="true"
            />
            <CopyButton :text="workspaceId" label="Copy workspace ID" />
          </div>
        </Field>
        <ErrorAlert :error="rename.error.value" />
        <template #footer>
          <SubmitButton
            :pending="rename.asyncStatus.value === 'loading'"
            :disabled="!workspace || name === workspace.name"
          >
            Save changes
          </SubmitButton>
        </template>
      </Card>
    </form>
  </Page>
</template>
