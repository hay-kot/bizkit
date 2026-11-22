<script setup lang="ts">
import { FolderKanban, Plus, Trash2 } from 'lucide-vue-next'
import { ref, watch } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PageHeader from '@/components/PageHeader.vue'
import Field from '@/components/ui/Field.vue'
import Overlay from '@/components/ui/Overlay.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { confirmAction } from '@/lib/confirm'
import { errorMessage } from '@/lib/errors'
import { slugify } from '@/lib/slug'
import { toast } from '@/lib/toast'
import type { Project } from '@/queries/projects'
import { useCreateProject, useDeleteProject, useProjects } from '@/queries/projects'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: projects, isPending, error } = useProjects(() => props.workspaceId)
const create = useCreateProject()
const remove = useDeleteProject()

const panelOpen = ref(false)
const name = ref('')
const slug = ref('')
const slugEdited = ref(false)
watch(name, (value) => {
  if (!slugEdited.value) slug.value = slugify(value)
})

function openPanel() {
  name.value = ''
  slug.value = ''
  slugEdited.value = false
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  const project = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, slug: slug.value },
  })
  panelOpen.value = false
  toast(`Created ${project.name}`)
}

async function onDelete(project: Project) {
  const ok = await confirmAction({
    title: `Delete ${project.name}?`,
    message: 'This removes the project and everything in it. You can’t undo this.',
    confirmLabel: 'Delete project',
  })
  if (!ok) return
  try {
    await remove.mutateAsync({ path: { workspace_id: props.workspaceId, project_id: project.id } })
    toast(`Deleted ${project.name}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}

function formatDate(value: string) {
  return new Date(value).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
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

    <SkeletonRows v-if="isPending" />
    <ErrorAlert v-else-if="error" :error="error" />
    <EmptyState
      v-else-if="!projects?.items.length"
      :icon="FolderKanban"
      title="No projects yet"
      message="Create a project for each site or app you run."
    >
      <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New project</button>
    </EmptyState>
    <div v-else class="overflow-x-auto rounded-box border border-base-300">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Slug</th>
            <th scope="col">Created</th>
            <th scope="col"><span class="sr-only">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="project in projects.items"
            :key="project.id"
            class="transition-colors hover:bg-base-200"
          >
            <td class="font-medium">{{ project.name }}</td>
            <td>
              <code class="text-sm">{{ project.slug }}</code>
            </td>
            <td class="text-sm">{{ formatDate(project.created_at) }}</td>
            <td class="text-right">
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :aria-label="`Delete ${project.name}`"
                @click="onDelete(project)"
              >
                <Trash2 class="size-4" aria-hidden="true" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <Overlay :open="panelOpen" variant="panel" title="New project" @close="panelOpen = false">
      <form id="new-project" class="flex flex-col gap-4" @submit.prevent="onCreate">
        <Field label="Name">
          <input
            v-model="name"
            name="project-name"
            required
            class="input w-full"
            placeholder="Website"
          />
        </Field>
        <Field label="Slug" hint="Lowercase letters, digits and dashes. Unique in this workspace.">
          <input
            v-model="slug"
            name="project-slug"
            required
            class="input w-full"
            placeholder="website"
            @input="slugEdited = true"
          />
        </Field>
        <ErrorAlert :error="create.error.value" />
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost btn-sm" @click="panelOpen = false">
          Cancel
        </button>
        <button
          type="submit"
          form="new-project"
          class="btn btn-primary btn-sm"
          :disabled="create.asyncStatus.value === 'loading'"
        >
          <span
            v-if="create.asyncStatus.value === 'loading'"
            class="loading loading-xs loading-spinner"
          ></span>
          Create project
        </button>
      </template>
    </Overlay>
  </div>
</template>
