<script setup lang="ts">
import { Mail, Plus, Settings } from 'lucide-vue-next'
import { onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Avatar from '@/components/ui/Avatar.vue'
import Field from '@/components/ui/Field.vue'
import Overlay from '@/components/ui/Overlay.vue'
import { errorMessage } from '@/lib/errors'
import { slugify } from '@/lib/slug'
import { toast } from '@/lib/toast'
import { useMe } from '@/queries/auth'
import type { MyInvitation } from '@/queries/workspaces'
import {
  useAcceptInvitation,
  useCreateWorkspace,
  useDeclineInvitation,
  useMyInvitations,
} from '@/queries/workspaces'

const route = useRoute()
const router = useRouter()
const { data: me, isPending } = useMe()
const { data: invitations } = useMyInvitations()
const create = useCreateWorkspace()
const accept = useAcceptInvitation()
const decline = useDeclineInvitation()

const creating = ref(false)
const name = ref('')
const slug = ref('')
const slugEdited = ref(false)
watch(name, (value) => {
  if (!slugEdited.value) slug.value = slugify(value)
})

function openCreate() {
  name.value = ''
  slug.value = ''
  slugEdited.value = false
  create.reset()
  creating.value = true
}

// The workspace switcher links here with ?new=1.
onMounted(() => {
  if (route.query.new) {
    openCreate()
    void router.replace({ query: {} })
  }
})

async function onCreate() {
  const workspace = await create.mutateAsync({ body: { name: name.value, slug: slug.value } })
  creating.value = false
  await router.push(`/w/${workspace.id}/projects`)
}

async function onAccept(invite: MyInvitation) {
  try {
    const membership = await accept.mutateAsync({ path: { invitation_id: invite.id } })
    toast(`You joined ${membership.workspace.name}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}

async function onDecline(invite: MyInvitation) {
  await decline.mutateAsync({ path: { invitation_id: invite.id } })
  toast(`Declined ${invite.workspace.name}`, 'info')
}
</script>

<template>
  <div class="flex flex-col gap-8">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">Your workspaces</h1>
      <p class="mt-1 text-sm text-base-content/80">
        Pick a workspace to work in, or start a new one.
      </p>
    </div>

    <section
      v-if="invitations?.items.length"
      aria-labelledby="invites-heading"
      class="flex flex-col gap-3"
    >
      <h2 id="invites-heading" class="text-sm font-medium">Invitations</h2>
      <TransitionGroup
        tag="ul"
        class="flex flex-col gap-2"
        leave-active-class="transition duration-150 ease-in"
        leave-to-class="opacity-0"
      >
        <li
          v-for="invite in invitations.items"
          :key="invite.id"
          class="flex flex-wrap items-center gap-3 rounded-box border border-base-300 bg-base-100 p-3"
        >
          <span class="inline-flex size-8 items-center justify-center rounded-field bg-base-200">
            <Mail class="size-4" aria-hidden="true" />
          </span>
          <span class="min-w-0 flex-1 text-sm">
            <strong>{{ invite.workspace.name }}</strong>
            <span v-if="invite.invited_by"> · from {{ invite.invited_by }}</span>
            <span> · as {{ invite.role }}</span>
          </span>
          <span class="flex gap-2">
            <button type="button" class="btn btn-ghost btn-sm" @click="onDecline(invite)">
              Decline
            </button>
            <button type="button" class="btn btn-primary btn-sm" @click="onAccept(invite)">
              Accept
            </button>
          </span>
        </li>
      </TransitionGroup>
    </section>

    <section aria-labelledby="workspaces-heading">
      <h2 id="workspaces-heading" class="sr-only">Workspaces</h2>
      <div v-if="isPending" class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <div v-for="n in 3" :key="n" class="h-24 animate-pulse rounded-box bg-base-300"></div>
      </div>
      <ul v-else class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <li v-for="m in me?.memberships ?? []" :key="m.workspace.id" class="relative">
          <RouterLink
            :to="`/w/${m.workspace.id}/projects`"
            class="flex h-full items-start gap-3 rounded-box border border-base-300 bg-base-100 p-4 transition hover:border-base-content/40 hover:shadow-sm"
          >
            <Avatar :name="m.workspace.name" size="lg" />
            <span class="flex min-w-0 flex-col">
              <span class="truncate font-medium">{{ m.workspace.name }}</span>
              <span class="truncate text-sm text-base-content/70">{{ m.workspace.slug }}</span>
              <span class="mt-2 text-xs capitalize">{{ m.role }}</span>
            </span>
          </RouterLink>
          <RouterLink
            v-if="m.role === 'admin'"
            :to="`/w/${m.workspace.id}/settings`"
            class="btn absolute top-2 right-2 btn-square btn-ghost btn-sm"
            :aria-label="`Settings for ${m.workspace.name}`"
          >
            <Settings class="size-4" aria-hidden="true" />
          </RouterLink>
        </li>
        <li>
          <button
            type="button"
            class="flex h-full min-h-24 w-full items-center justify-center gap-2 rounded-box border border-dashed border-base-300 p-4 text-sm font-medium transition hover:border-base-content/40 hover:bg-base-100"
            @click="openCreate"
          >
            <Plus class="size-4" aria-hidden="true" />New workspace
          </button>
        </li>
      </ul>
    </section>

    <Overlay
      :open="creating"
      title="New workspace"
      description="You'll be its admin. Invite others once it exists."
      @close="creating = false"
    >
      <form id="new-workspace" class="flex flex-col gap-4" @submit.prevent="onCreate">
        <Field label="Name">
          <input v-model="name" name="name" required class="input w-full" placeholder="Acme" />
        </Field>
        <Field label="Slug" hint="Used in policies and links. It can't change later.">
          <input
            v-model="slug"
            name="slug"
            required
            class="input w-full"
            placeholder="acme"
            @input="slugEdited = true"
          />
        </Field>
        <ErrorAlert :error="create.error.value" />
      </form>
      <template #footer>
        <button type="button" class="btn btn-ghost btn-sm" @click="creating = false">Cancel</button>
        <button
          type="submit"
          form="new-workspace"
          class="btn btn-primary btn-sm"
          :disabled="create.asyncStatus.value === 'loading'"
        >
          <span
            v-if="create.asyncStatus.value === 'loading'"
            class="loading loading-xs loading-spinner"
          ></span>
          Create workspace
        </button>
      </template>
    </Overlay>
  </div>
</template>
