<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import { UserRound } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import IdentityNeeded from '@/components/analytics/IdentityNeeded.vue'
import DataTable from '@/components/ui/DataTable.vue'
import { formatDateTime, formatNumber, formatRelative } from '@/lib/format'
import { PEOPLE_SHOWN, usePeople } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const search = ref('')
const query = refDebounced(search, 250)
const {
  data: people,
  isPending,
  error,
} = usePeople(
  () => props.workspaceId,
  () => props.projectId,
  query,
)
const searching = computed(() => query.value.trim() !== '')

const columns = [
  { key: 'person', label: 'Person' },
  { key: 'first', label: 'First seen' },
  { key: 'last', label: 'Last seen' },
  { key: 'events', label: 'Events', class: 'text-right' },
]

function personPath(personId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/analytics/people/${personId}`
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="People"
    description="Users your site identified with bizkit.identify, most recently seen first."
  >
    <template #default="{ site }">
      <IdentityNeeded
        v-if="
          site.identity_mode === 'cookieless' && !searching && !isPending && !people?.items.length
        "
        :workspace-id="workspaceId"
        :project-id="projectId"
        what="People"
      />
      <template v-else>
        <input
          v-model="search"
          type="search"
          name="people-search"
          class="input w-full sm:w-80"
          placeholder="Search by user ID, email or name"
          aria-label="Search people"
        />
        <DataTable
          :columns="columns"
          :rows="people?.items"
          :row-key="(p) => p.id"
          :loading="isPending"
          :error="error"
          :empty="
            searching
              ? { title: 'Nobody matches', message: 'Try another ID, email or name.' }
              : {
                  icon: UserRound,
                  title: 'No people yet',
                  message: 'People show up once your site calls bizkit.identify(userId, traits).',
                }
          "
        >
          <template #cell-person="{ row }">
            <RouterLink :to="personPath(row.id)" class="link font-medium link-hover">
              {{ row.display ?? row.user_id }}
            </RouterLink>
            <span v-if="row.display" class="ms-2 text-sm text-base-content/70">{{
              row.user_id
            }}</span>
          </template>
          <template #cell-first="{ row }">
            <span class="text-sm">{{ formatDateTime(row.first_seen) }}</span>
          </template>
          <template #cell-last="{ row }">
            <span class="text-sm">{{ formatRelative(row.last_seen) }}</span>
          </template>
          <template #cell-events="{ row }">
            <span class="text-sm tabular-nums">{{ formatNumber(row.events) }}</span>
          </template>
        </DataTable>
        <p v-if="(people?.items.length ?? 0) >= PEOPLE_SHOWN" class="text-sm text-base-content/70">
          Showing the {{ PEOPLE_SHOWN }} most recently seen. Search to find others.
        </p>
      </template>
    </template>
  </AnalyticsShell>
</template>
