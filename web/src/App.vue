<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { useMe, useSignOut } from '@/queries/auth'

const route = useRoute()
const router = useRouter()
const isPublic = computed(() => route.meta.public === true)
const { data: me } = useMe(() => !isPublic.value)
const { mutateAsync: signOut } = useSignOut()

async function onSignOut() {
  await signOut({})
  await router.replace('/sign-in')
}
</script>

<template>
  <div class="flex min-h-screen flex-col bg-base-200">
    <header class="navbar border-b border-base-300 bg-base-100 px-4">
      <div class="flex-1">
        <RouterLink to="/" class="text-lg font-semibold">bizkit</RouterLink>
      </div>
      <div v-if="!isPublic && me" class="flex items-center gap-3">
        <span class="hidden text-sm sm:inline" data-testid="signed-in-as">{{ me.email }}</span>
        <button type="button" class="btn btn-ghost btn-sm" @click="onSignOut">Sign out</button>
      </div>
    </header>
    <main class="mx-auto w-full max-w-5xl flex-1 p-4 sm:p-6">
      <RouterView />
    </main>
  </div>
</template>
