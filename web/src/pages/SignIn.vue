<script setup lang="ts">
import { ref } from 'vue'
import { useRoute } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import { useSignIn } from '@/queries/auth'

const route = useRoute()
const email = ref(typeof route.query.email === 'string' ? route.query.email : '')
const next = typeof route.query.next === 'string' ? route.query.next : undefined
const sentTo = ref('')
const { mutateAsync, asyncStatus, error } = useSignIn()

async function submit() {
  await mutateAsync({ body: { email: email.value, next } })
  sentTo.value = email.value
}
</script>

<template>
  <div class="card mx-auto mt-12 max-w-md bg-base-100">
    <div class="card-body gap-4">
      <h1 class="card-title text-2xl">Sign in to bizkit</h1>
      <div v-if="sentTo" role="status" class="alert alert-success" data-testid="link-sent">
        <span>Check {{ sentTo }} for a sign-in link. It expires in 15 minutes.</span>
      </div>
      <form v-else class="flex flex-col gap-4" @submit.prevent="submit">
        <p class="text-sm">We'll email you a link. No password needed.</p>
        <label class="flex flex-col gap-1">
          <span class="text-sm font-medium">Email</span>
          <input
            v-model="email"
            type="email"
            name="email"
            required
            autocomplete="email"
            class="input w-full"
            placeholder="you@example.com"
          />
        </label>
        <ErrorAlert :error="error" />
        <button type="submit" class="btn btn-primary" :disabled="asyncStatus === 'loading'">
          Email me a link
        </button>
      </form>
    </div>
  </div>
</template>
