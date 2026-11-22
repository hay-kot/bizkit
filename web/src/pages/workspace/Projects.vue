<script setup lang="ts">
import { ref, watch } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import { slugify } from '@/lib/slug'
import { useCreateProject, useDeleteProject, useProjects } from '@/queries/projects'

const props = defineProps<{ workspaceId: string }>()
const { data: projects, isPending, error } = useProjects(() => props.workspaceId)
const create = useCreateProject()
const remove = useDeleteProject()

const name = ref('')
const slug = ref('')
watch(name, (value) => (slug.value = slugify(value)))

async function onCreate() {
  await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, slug: slug.value },
  })
  name.value = ''
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <section class="card bg-base-100" aria-labelledby="projects-heading">
      <div class="card-body">
        <h2 id="projects-heading" class="card-title text-lg">Projects</h2>
        <Loading v-if="isPending" />
        <ErrorAlert v-else-if="error" :error="error" />
        <EmptyState v-else-if="!projects?.items.length" message="No projects yet." />
        <div v-else class="overflow-x-auto">
          <table class="table">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Slug</th>
                <th scope="col"><span class="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="project in projects.items" :key="project.id">
                <td class="font-medium">{{ project.name }}</td>
                <td>
                  <code>{{ project.slug }}</code>
                </td>
                <td class="text-right">
                  <button
                    type="button"
                    class="btn btn-ghost btn-xs"
                    :aria-label="`Delete ${project.name}`"
                    @click="
                      remove.mutate({ path: { workspace_id: workspaceId, project_id: project.id } })
                    "
                  >
                    Delete
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>
    <section class="card bg-base-100" aria-labelledby="new-project-heading">
      <form class="card-body gap-4" @submit.prevent="onCreate">
        <h2 id="new-project-heading" class="card-title text-lg">New project</h2>
        <div class="grid gap-4 sm:grid-cols-2">
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium">Name</span>
            <input
              v-model="name"
              name="project-name"
              required
              class="input w-full"
              placeholder="Website"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium">Slug</span>
            <input
              v-model="slug"
              name="project-slug"
              required
              class="input w-full"
              placeholder="website"
            />
          </label>
        </div>
        <ErrorAlert :error="create.error.value" />
        <div class="card-actions justify-end">
          <button type="submit" class="btn btn-primary">Create project</button>
        </div>
      </form>
    </section>
  </div>
</template>
