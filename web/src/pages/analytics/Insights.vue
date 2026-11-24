<script setup lang="ts">
import { LayoutDashboard, Lightbulb, Plus } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import { describeQuery } from '@/components/analytics/insightDraft'
import {
  ANY_DASHBOARD,
  filterInsights,
  knownDashboardFilter,
  NO_DASHBOARD,
} from '@/components/analytics/insightList'
import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import CardGrid from '@/components/ui/CardGrid.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import LinkCard from '@/components/ui/LinkCard.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select, { type SelectOption } from '@/components/ui/Select.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatRelative } from '@/lib/format'
import { useCreateDashboard, useDashboards, useInsights } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const insights = useInsights(
  () => props.workspaceId,
  () => props.projectId,
)
const dashboards = useDashboards(
  () => props.workspaceId,
  () => props.projectId,
)
const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics`)

const search = ref('')
const onDashboard = ref<string>(ANY_DASHBOARD)
const dashboardOptions = computed((): SelectOption<string>[] => [
  { value: ANY_DASHBOARD, label: 'Any dashboard' },
  { value: NO_DASHBOARD, label: 'No dashboard' },
  ...(dashboards.data.value?.items ?? []).map((d) => ({ value: d.id, label: d.name })),
])
watch(
  () => dashboards.data.value?.items,
  (items) => {
    onDashboard.value = knownDashboardFilter(
      onDashboard.value,
      items?.map((d) => d.id),
    )
  },
)
const filtering = computed(() => search.value.trim() !== '' || onDashboard.value !== ANY_DASHBOARD)
const shownInsights = computed(() => {
  const items = insights.data.value?.items
  return items && filterInsights(items, search.value, onDashboard.value)
})

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'query', label: 'Query' },
  { key: 'dashboards', label: 'Dashboards' },
  { key: 'updated', label: 'Updated', class: 'whitespace-nowrap' },
]

const create = useCreateDashboard()
const panelOpen = ref(false)
const name = ref('')
// Insights are a page of their own (`insights/new`), so the one create panel
// here, and the one `?new=1` opens, is the dashboard's.
useCreateIntent(openPanel)

function openPanel() {
  name.value = ''
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  const board = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { project_id: props.projectId, name: name.value },
  })
  panelOpen.value = false
  await router.push(`${base.value}/dashboards/${board.id}`)
}

function insightCount(n: number): string {
  return n === 1 ? '1 insight' : `${n} insights`
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Insights"
    description="Saved questions about your events and people, and dashboards that show them together."
  >
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New dashboard
      </button>
      <RouterLink :to="`${base}/insights/new`" class="btn gap-1.5 btn-primary btn-sm">
        <Plus class="size-4" aria-hidden="true" />New insight
      </RouterLink>
    </template>

    <PageSection title="Dashboards">
      <ErrorAlert v-if="dashboards.error.value" :error="dashboards.error.value" />
      <EmptyState
        v-else-if="dashboards.data.value?.items.length === 0"
        :icon="LayoutDashboard"
        title="No dashboards yet"
        message="A dashboard shows saved insights side by side, over one date range."
      >
        <button type="button" class="btn btn-sm" @click="openPanel">New dashboard</button>
      </EmptyState>
      <CardGrid
        v-else
        :items="dashboards.data.value?.items"
        :item-key="(d) => d.id"
        :loading="dashboards.isPending.value"
      >
        <template #item="{ item }">
          <LinkCard :to="`${base}/dashboards/${item.id}`" :title="item.name">
            <template #leading>
              <span
                class="inline-flex size-9 shrink-0 items-center justify-center rounded-box bg-base-200"
              >
                <LayoutDashboard class="size-4 text-base-content/70" aria-hidden="true" />
              </span>
            </template>
            <template #meta>
              {{ insightCount(item.tiles.length) }} · Updated
              {{ formatRelative(item.updated_at) }}
            </template>
          </LinkCard>
        </template>
        <template #end>
          <button
            type="button"
            class="flex h-full min-h-20 w-full items-center justify-center gap-2 rounded-box border border-dashed border-base-300 p-4 text-sm font-medium transition hover:border-base-content/40 hover:bg-base-100"
            @click="openPanel"
          >
            <Plus class="size-4" aria-hidden="true" />New dashboard
          </button>
        </template>
      </CardGrid>
    </PageSection>

    <PageSection title="Insights">
      <template #actions>
        <input
          v-model="search"
          type="search"
          name="insight-search"
          aria-label="Search insights"
          placeholder="Search"
          class="input h-9 w-36 sm:w-48"
        />
        <div class="w-36 sm:w-44">
          <Select
            v-model="onDashboard"
            :options="dashboardOptions"
            label="On dashboard"
            size="toolbar"
          />
        </div>
      </template>
      <DataTable
        :columns="columns"
        :rows="shownInsights"
        :row-key="(i) => i.id"
        :row-to="(r) => `${base}/insights/${r.id}`"
        :loading="insights.isPending.value"
        :error="insights.error.value"
        label="Insights"
        :empty="
          filtering
            ? {
                icon: Lightbulb,
                title: 'No matching insights',
                message: 'Try another search or dashboard.',
              }
            : {
                icon: Lightbulb,
                title: 'No insights yet',
                message: 'An insight counts events or people, and you can pin it to a dashboard.',
              }
        "
      >
        <template #cell-name="{ row }">
          {{ row.name }}
        </template>
        <template #cell-query="{ row }">
          <span class="line-clamp-1 text-sm text-base-content/70">{{
            describeQuery(row.query)
          }}</span>
        </template>
        <template #cell-dashboards="{ row }">
          <span class="flex flex-wrap gap-1">
            <span
              v-for="d in row.dashboards"
              :key="d.id"
              class="badge badge-ghost badge-sm whitespace-nowrap"
              >{{ d.name }}</span
            >
          </span>
        </template>
        <template #cell-updated="{ row }">
          <span class="text-sm">{{ formatRelative(row.updated_at) }}</span>
        </template>
        <template v-if="!filtering" #empty-action>
          <RouterLink :to="`${base}/insights/new`" class="btn btn-primary btn-sm"
            >New insight</RouterLink
          >
        </template>
      </DataTable>
    </PageSection>

    <FormPanel
      :open="panelOpen"
      variant="dialog"
      title="New dashboard"
      submit-label="Create dashboard"
      :pending="create.isLoading.value"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input v-model="name" name="dashboard-name" class="input w-full" required maxlength="100" />
      </Field>
    </FormPanel>
  </AnalyticsShell>
</template>
