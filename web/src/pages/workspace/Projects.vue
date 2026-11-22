<script setup lang="ts">
import { FolderKanban, Plus, Trash2 } from 'lucide-vue-next'
import { ref } from 'vue'

import PageHeader from '@/components/PageHeader.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useSlugFromName } from '@/composables/useSlugFromName'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import { toast } from '@/lib/toast'
import type { Project } from '@/queries/projects'
import { useCreateProject, useDeleteProject, useProjects } from '@/queries/projects'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: projects, isPending, error } = useProjects(() => props.workspaceId)
const create = useCreateProject()
const remove = useDeleteProject()
const form = useSlugFromName()
const panelOpen = ref(false)
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'slug', label: 'Slug' },
  { key: 'created', label: 'Created' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-12 text-right' },
]

function openPanel() {
  form.reset()
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  const project = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: form.name.value, slug: form.slug.value },
  })
  panelOpen.value = false
  toast(`Created ${project.name}`)
}

function onDelete(project: Project) {
  return runAction({
    confirm: {
      title: `Delete ${project.name}?`,
      message:
        'This removes the project and everything in it, such as its forms and their submissions. You can’t undo this.',
      confirmLabel: 'Delete project',
    },
    run: () =>
      remove.mutateAsync({ path: { workspace_id: props.workspaceId, project_id: project.id } }),
    success: `Deleted ${project.name}`,
  })
}
</script>

<template>
  <div>
    <PageHeader
      :section="workspaceName"
      title="Projects"
      description="Everything in a workspace hangs off a project."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New project
      </button>
    </PageHeader>

    <DataTable
      :columns="columns"
      :rows="projects?.items"
      :row-key="(p) => p.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: FolderKanban,
        title: 'No projects yet',
        message: 'Create a project for each site or app you run.',
      }"
    >
      <template #cell-name="{ row }"
        ><span class="font-medium">{{ row.name }}</span></template
      >
      <template #cell-slug="{ row }"
        ><code class="text-sm">{{ row.slug }}</code></template
      >
      <template #cell-created="{ row }"
        ><span class="text-sm">{{ formatDate(row.created_at) }}</span></template
      >
      <template #cell-actions="{ row }">
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Delete ${row.name}`"
          @click="onDelete(row)"
        >
          <Trash2 class="size-4" aria-hidden="true" />
        </button>
      </template>
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New project</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New project"
      submit-label="Create project"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input
          v-model="form.name.value"
          name="project-name"
          required
          class="input w-full"
          placeholder="Website"
        />
      </Field>
      <Field label="Slug" hint="Lowercase letters, digits and dashes. Unique in this workspace.">
        <input
          v-model="form.slug.value"
          name="project-slug"
          required
          class="input w-full"
          placeholder="website"
          @input="form.onSlugInput"
        />
      </Field>
    </FormPanel>
  </div>
</template>
