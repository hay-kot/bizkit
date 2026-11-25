<script setup lang="ts">
import { ChartLine } from 'lucide-vue-next'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Card from '@/components/ui/Card.vue'
import CodeBlock from '@/components/ui/CodeBlock.vue'
import Spinner from '@/components/ui/Spinner.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { type Site, useCreateSite } from '@/queries/analytics'

/**
 * First run: a button that turns analytics on, then the snippet to paste
 * while the project waits for its first pageview.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  site: Site | undefined
}>()
const isAdmin = useIsAdmin(() => props.workspaceId)
const create = useCreateSite()

function turnOn() {
  void create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { project_id: props.projectId },
  })
}
</script>

<template>
  <Card
    v-if="!site"
    :title="`Set up analytics for ${projectName}`"
    description="Count visitors, pageviews and custom events from one small script. It sets no cookies unless you choose to."
  >
    <div class="flex items-center gap-3">
      <span class="inline-flex size-10 items-center justify-center rounded-full bg-base-200">
        <ChartLine class="size-5" aria-hidden="true" />
      </span>
      <p v-if="isAdmin" class="text-sm">Turning it on gives this project its tracking snippet.</p>
      <p v-else class="text-sm">Ask a workspace admin to turn on analytics for this project.</p>
    </div>
    <ErrorAlert :error="create.error.value" />
    <template v-if="isAdmin" #footer>
      <form @submit.prevent="turnOn">
        <SubmitButton :pending="create.asyncStatus.value === 'loading'">
          Turn on analytics
        </SubmitButton>
      </form>
    </template>
  </Card>
  <Card
    v-else
    :title="`Set up analytics for ${projectName}`"
    description="Add this to the <head> of every page on your site."
  >
    <CodeBlock
      :code="site.snippet"
      language="html"
      copy="Copy snippet"
      data-testid="analytics-snippet"
    />
    <p class="text-sm text-base-content/70">
      {{
        site.hostnames.length
          ? `Only ${site.hostnames.join(', ')} can send events.`
          : 'Any hostname can send events. Limit them in Settings.'
      }}
    </p>
    <p class="flex items-center gap-2 text-sm">
      <Spinner label="Waiting" />
      Waiting for the first pageview...
    </p>
  </Card>
</template>
