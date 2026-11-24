<script setup lang="ts">
import { Eye, Server, Zap } from 'lucide-vue-next'
import { computed, watch } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import List from '@/components/ui/List.vue'
import ListItem from '@/components/ui/ListItem.vue'
import PageSection from '@/components/ui/PageSection.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { formatDateTime, formatUtcDay } from '@/lib/format'
import { type EventItem, TIMELINE_SHOWN, usePerson } from '@/queries/analytics'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  personId: string
}>()
const router = useRouter()
const {
  data: person,
  isPending,
  error,
} = usePerson(
  () => props.workspaceId,
  () => props.personId,
)
watch(person, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/analytics/people/${loaded.id}`,
    )
  }
})

function traitText(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value)
}

const title = computed(() => {
  const traits = person.value?.traits ?? {}
  const display = traits.email ?? traits.name
  return typeof display === 'string' ? display : (person.value?.user_id ?? 'Person')
})

const facts = computed(() => {
  const p = person.value
  if (!p) return []
  return [
    { key: 'user_id', label: 'User ID', value: p.user_id },
    { key: 'first', label: 'First seen', value: formatDateTime(p.first_seen) },
    { key: 'last', label: 'Last seen', value: formatDateTime(p.last_seen) },
    ...Object.entries(p.traits).map(([key, value]) => ({
      key: `trait:${key}`,
      label: key,
      value: traitText(value),
    })),
  ]
})

/** Events grouped by UTC day, newest day first, as the API orders them. */
const days = computed(() => {
  const groups: { day: string; events: EventItem[] }[] = []
  for (const event of person.value?.events ?? []) {
    const day = event.ts.slice(0, 10)
    const last = groups[groups.length - 1]
    if (last?.day === day) last.events.push(event)
    else groups.push({ day, events: [event] })
  }
  return groups
})

function icon(event: EventItem) {
  if (event.name === 'pageview') return Eye
  return event.source === 'server' ? Server : Zap
}

function eventTitle(event: EventItem): string {
  return event.name === 'pageview' ? `Viewed ${event.path ?? 'a page'}` : event.name
}

function eventDetail(event: EventItem): string | undefined {
  const parts = Object.entries(event.props).map(([key, value]) => `${key}=${traitText(value)}`)
  if (event.name !== 'pageview' && event.path) parts.unshift(event.path)
  return parts.length ? parts.join(', ') : undefined
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    :title="title"
    description="Traits from identify, then every stored event, newest first."
  >
    <ErrorAlert v-if="error" :error="error" />
    <SkeletonRows v-else-if="isPending" :rows="6" />
    <template v-else-if="person">
      <DescriptionList :items="facts" />
      <p v-if="!days.length" class="text-sm text-base-content/70">No stored events.</p>
      <PageSection v-for="group in days" :key="group.day" :title="formatUtcDay(group.day)">
        <List
          :items="group.events"
          :item-key="(e) => String(e.id)"
          :label="formatUtcDay(group.day)"
        >
          <template #item="{ item }">
            <ListItem :title="eventTitle(item)" :description="eventDetail(item)">
              <template #leading>
                <component
                  :is="icon(item)"
                  class="size-4 shrink-0 text-base-content/70"
                  aria-hidden="true"
                />
              </template>
              <template #meta
                ><span class="tabular-nums">{{ item.ts.slice(11, 16) }} UTC</span></template
              >
            </ListItem>
          </template>
        </List>
      </PageSection>
      <p v-if="(person.events.length ?? 0) >= TIMELINE_SHOWN" class="text-sm text-base-content/70">
        Showing the latest {{ TIMELINE_SHOWN }} events.
      </p>
    </template>
  </AnalyticsShell>
</template>
