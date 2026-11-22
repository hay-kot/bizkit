<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { useVerify } from '@/queries/auth'

const route = useRoute()
const router = useRouter()
const token = typeof route.query.token === 'string' ? route.query.token : ''
const next =
  typeof route.query.next === 'string' && route.query.next.startsWith('/') ? route.query.next : '/'
const failed = ref(!token)
const { mutateAsync } = useVerify()

// The link opens this page and the page POSTs the token, so mail scanners
// that prefetch links never use it up.
onMounted(async () => {
  if (!token) return
  try {
    await mutateAsync({ body: { token } })
    await router.replace(next)
  } catch {
    failed.value = true
  }
})
</script>

<template>
  <div class="card mx-auto mt-12 max-w-md bg-base-100">
    <div class="card-body items-center gap-4 text-center">
      <template v-if="failed">
        <h1 class="card-title">This link doesn't work</h1>
        <p class="text-sm">It was already used or has expired. Request a new one.</p>
        <RouterLink to="/sign-in" class="btn btn-primary btn-sm">Back to sign-in</RouterLink>
      </template>
      <template v-else>
        <h1 class="card-title">Signing you in</h1>
        <span class="loading loading-spinner" aria-label="Signing in"></span>
      </template>
    </div>
  </div>
</template>
