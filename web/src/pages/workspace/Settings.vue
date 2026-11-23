<script setup lang="ts">
import { ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import Field from '@/components/ui/Field.vue'
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
  <div>
    <PageHeader :section="workspaceName" title="Settings" />
    <section class="grid gap-6 md:grid-cols-3" aria-labelledby="general-heading">
      <div>
        <h2 id="general-heading" class="font-medium">General</h2>
        <p class="mt-1 text-sm text-base-content/80">
          The name shows in the switcher and in invitation emails.
        </p>
      </div>
      <form
        class="flex flex-col gap-4 rounded-box border border-base-300 p-5 md:col-span-2"
        @submit.prevent="onSave"
      >
        <Field label="Name">
          <input v-model="name" name="workspace-name" required class="input w-full" />
        </Field>
        <Field label="Slug" hint="The slug never changes: email policies join workspaces by slug.">
          <div class="flex items-center gap-2">
            <input
              :value="workspace?.slug"
              readonly
              class="input w-full bg-base-200"
              aria-readonly="true"
            />
            <CopyButton v-if="workspace" :text="workspace.slug" label="Copy slug" />
          </div>
        </Field>
        <Field label="Workspace ID">
          <div class="flex items-center gap-2">
            <input
              :value="workspaceId"
              readonly
              class="input w-full bg-base-200 font-mono text-sm"
              aria-readonly="true"
            />
            <CopyButton :text="workspaceId" label="Copy workspace ID" />
          </div>
        </Field>
        <ErrorAlert :error="rename.error.value" />
        <div class="flex justify-end">
          <SubmitButton
            :pending="rename.asyncStatus.value === 'loading'"
            :disabled="name === workspace?.name"
          >
            Save changes
          </SubmitButton>
        </div>
      </form>
    </section>
  </div>
</template>
