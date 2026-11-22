<script setup lang="ts">
import { ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import { useRenameWorkspace, useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const { data: workspace } = useWorkspace(() => props.workspaceId)
const rename = useRenameWorkspace()
const name = ref('')
const saved = ref(false)
watch(workspace, (value) => (name.value = value?.name ?? ''), { immediate: true })

async function onSave() {
  saved.value = false
  await rename.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value },
  })
  saved.value = true
}
</script>

<template>
  <section class="card bg-base-100" aria-labelledby="settings-heading">
    <form class="card-body gap-4" @submit.prevent="onSave">
      <h2 id="settings-heading" class="card-title text-lg">Settings</h2>
      <label class="flex flex-col gap-1">
        <span class="text-sm font-medium">Name</span>
        <input v-model="name" name="workspace-name" required class="input w-full" />
      </label>
      <p class="text-sm">
        Slug: <code>{{ workspace?.slug }}</code> (it never changes, because email policies join
        workspaces by slug)
      </p>
      <div v-if="saved" role="status" class="alert alert-success"><span>Saved.</span></div>
      <ErrorAlert :error="rename.error.value" />
      <div class="card-actions justify-end">
        <button type="submit" class="btn btn-primary">Save</button>
      </div>
    </form>
  </section>
</template>
