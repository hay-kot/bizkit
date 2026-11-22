<script setup lang="ts">
import { MailCheck } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRoute } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Field from '@/components/ui/Field.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
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
  <div class="rounded-box border border-base-300 bg-base-100 p-8 shadow-sm">
    <Transition
      mode="out-in"
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="sentTo"
        class="flex flex-col items-center gap-3 text-center"
        role="status"
        data-testid="link-sent"
      >
        <span class="inline-flex size-12 items-center justify-center rounded-full bg-base-200">
          <MailCheck class="size-6" aria-hidden="true" />
        </span>
        <h1 class="text-lg font-semibold">Check your email</h1>
        <p class="text-sm text-base-content/80">
          We sent a sign-in link to {{ sentTo }}. It expires in 15 minutes.
        </p>
        <button type="button" class="btn btn-ghost btn-sm" @click="sentTo = ''">
          Use a different email
        </button>
      </div>
      <form v-else class="flex flex-col gap-5" @submit.prevent="submit">
        <div>
          <h1 class="text-xl font-semibold tracking-tight">Sign in to bizkit</h1>
          <p class="mt-1 text-sm text-base-content/80">
            We'll email you a link. No password needed.
          </p>
        </div>
        <Field label="Email">
          <input
            v-model="email"
            type="email"
            name="email"
            required
            autocomplete="email"
            class="input w-full"
            placeholder="you@example.com"
          />
        </Field>
        <ErrorAlert :error="error" />
        <SubmitButton size="md" :pending="asyncStatus === 'loading'">Email me a link</SubmitButton>
      </form>
    </Transition>
  </div>
</template>
