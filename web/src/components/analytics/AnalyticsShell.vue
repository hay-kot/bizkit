<script setup lang="ts">
import { useIntervalFn } from '@vueuse/core'
import { ChartLine } from 'lucide-vue-next'
import { computed, watch } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import TabPage from '@/components/ui/TabPage.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { isSiteMissing, type Site, useSite } from '@/queries/analytics'

import SetupCard from './SetupCard.vue'

/**
 * Every analytics page renders this inside `AnalyticsLayout`: its header
 * (title, description, actions) goes to the layout's header, and the page
 * shows once the project's site is known.
 *
 * - `setup` (Overview): the setup card until the first event arrives,
 *   polling while it waits.
 * - `events` (the other reports): an empty state that points to setup.
 * - `site` (Settings): the setup card only while analytics is off.
 */
const props = withDefaults(
  defineProps<{
    workspaceId: string
    projectId: string
    projectName: string
    gate: 'setup' | 'events' | 'site'
    title?: string
    description?: string
    width?: 'wide' | 'narrow'
  }>(),
  { title: 'Analytics', description: undefined, width: 'wide' },
)
defineSlots<{ default: (props: { site: Site }) => unknown; actions?: () => unknown }>()

const isAdmin = useIsAdmin(() => props.workspaceId)
const {
  data: site,
  error,
  isPending,
  refetch,
} = useSite(
  () => props.workspaceId,
  () => props.projectId,
)
const missing = computed(() => isSiteMissing(error.value))
const ready = computed(() => {
  if (!site.value) return false
  return props.gate === 'site' || site.value.has_events
})

const WAITING_POLL_MS = 5_000
const poll = useIntervalFn(() => void refetch(), WAITING_POLL_MS, { immediate: false })
watch(
  () => props.gate === 'setup' && site.value !== undefined && !site.value.has_events,
  (waiting) => (waiting ? poll.resume() : poll.pause()),
  { immediate: true },
)

const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics`)
</script>

<template>
  <TabPage :title="title" :description="description" :width="width">
    <template v-if="$slots.actions" #actions><slot name="actions" /></template>
    <SkeletonRows v-if="isPending" :rows="4" />
    <ErrorAlert v-else-if="error && !missing" :error="error" />
    <slot v-else-if="ready && site" :site="site" />
    <SetupCard
      v-else-if="gate !== 'events'"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :project-name="projectName"
      :site="site"
    />
    <EmptyState
      v-else
      :icon="ChartLine"
      :title="missing ? 'Analytics is off' : 'No events yet'"
      :message="
        missing
          ? 'Turn on analytics for this project to see reports here.'
          : 'Add the snippet to your site, and reports fill in as visitors arrive.'
      "
    >
      <RouterLink :to="isAdmin && !missing ? `${base}/settings` : base" class="btn btn-sm">
        {{ missing ? 'Set up analytics' : isAdmin ? 'Get the snippet' : 'Open the overview' }}
      </RouterLink>
    </EmptyState>
  </TabPage>
</template>
