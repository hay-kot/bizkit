<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import PageHeader from '@/components/PageHeader.vue'
import { slugify } from '@/lib/slug'
import { useMe } from '@/queries/auth'
import {
  useAcceptInvitation,
  useCreateWorkspace,
  useDeclineInvitation,
  useMyInvitations,
} from '@/queries/workspaces'

const router = useRouter()
const { data: me, isPending } = useMe()
const { data: invitations } = useMyInvitations()
const create = useCreateWorkspace()
const accept = useAcceptInvitation()
const decline = useDeclineInvitation()

const name = ref('')
const slug = ref('')
const slugEdited = ref(false)
watch(name, (value) => {
  if (!slugEdited.value) slug.value = slugify(value)
})

async function onCreate() {
  const workspace = await create.mutateAsync({ body: { name: name.value, slug: slug.value } })
  await router.push(`/w/${workspace.id}/projects`)
}
</script>

<template>
  <PageHeader
    title="Workspaces"
    subtitle="Each workspace has its own projects, members and API keys."
  />
  <Loading v-if="isPending" />
  <div v-else class="flex flex-col gap-6">
    <section
      v-if="invitations?.items.length"
      class="card bg-base-100"
      aria-labelledby="invites-heading"
    >
      <div class="card-body">
        <h2 id="invites-heading" class="card-title text-lg">Invitations</h2>
        <ul class="divide-y divide-base-300">
          <li
            v-for="invite in invitations.items"
            :key="invite.id"
            class="flex flex-wrap items-center justify-between gap-2 py-3"
          >
            <span>
              <strong>{{ invite.workspace.name }}</strong> as {{ invite.role }}
            </span>
            <span class="flex gap-2">
              <button
                type="button"
                class="btn btn-ghost btn-sm"
                @click="decline.mutate({ path: { invitation_id: invite.id } })"
              >
                Decline
              </button>
              <button
                type="button"
                class="btn btn-primary btn-sm"
                @click="accept.mutate({ path: { invitation_id: invite.id } })"
              >
                Accept
              </button>
            </span>
          </li>
        </ul>
      </div>
    </section>

    <section class="card bg-base-100" aria-labelledby="mine-heading">
      <div class="card-body">
        <h2 id="mine-heading" class="card-title text-lg">Your workspaces</h2>
        <EmptyState
          v-if="!me?.memberships.length"
          message="You're not in any workspace yet. Create one below."
        />
        <ul v-else class="divide-y divide-base-300">
          <li v-for="membership in me.memberships" :key="membership.workspace.id" class="py-3">
            <RouterLink
              :to="`/w/${membership.workspace.id}/projects`"
              class="link font-medium link-hover"
            >
              {{ membership.workspace.name }}
            </RouterLink>
            <span class="ml-2 badge badge-ghost badge-sm">{{ membership.role }}</span>
          </li>
        </ul>
      </div>
    </section>

    <section class="card bg-base-100" aria-labelledby="create-heading">
      <form class="card-body gap-4" @submit.prevent="onCreate">
        <h2 id="create-heading" class="card-title text-lg">Create a workspace</h2>
        <div class="grid gap-4 sm:grid-cols-2">
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium">Name</span>
            <input v-model="name" name="name" required class="input w-full" placeholder="Acme" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium">Slug</span>
            <input
              v-model="slug"
              name="slug"
              required
              class="input w-full"
              placeholder="acme"
              @input="slugEdited = true"
            />
          </label>
        </div>
        <ErrorAlert :error="create.error.value" />
        <div class="card-actions justify-end">
          <button
            type="submit"
            class="btn btn-primary"
            :disabled="create.asyncStatus.value === 'loading'"
          >
            Create workspace
          </button>
        </div>
      </form>
    </section>
  </div>
</template>
