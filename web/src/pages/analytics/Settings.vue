<script setup lang="ts">
import { Plus, Target, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import GoalPanel from '@/components/analytics/GoalPanel.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Card from '@/components/ui/Card.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { runAction } from '@/lib/action'
import {
  type Goal,
  type GoalKind,
  type IdentityMode,
  type Site,
  useCreateGoal,
  useDeleteGoal,
  useGoals,
  useSite,
  useUpdateSite,
} from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const route = useRoute()
const router = useRouter()
const { data: site } = useSite(
  () => props.workspaceId,
  () => props.projectId,
)
const update = useUpdateSite()

const TRACKING_EXAMPLE = `bizkit.track('signup_started', { plan: 'pro' })
bizkit.identify('user_123', { email: 'ada@example.com' })`

const identityModes: { value: IdentityMode; label: string; description: string }[] = [
  {
    value: 'cookieless',
    label: 'Cookieless',
    description: 'Stores nothing in the browser. Visitors are a daily hash; identify is ignored.',
  },
  {
    value: 'after_identify',
    label: 'After identify',
    description: 'Stores an anonymous ID once the site calls identify.',
  },
  {
    value: 'always',
    label: 'Always',
    description: 'Stores an anonymous ID from the first pageview, for sites that collect consent.',
  },
]

const hostnames = ref('')
const identityMode = ref<IdentityMode>('cookieless')
function load(current: Site | undefined) {
  if (!current) return
  hostnames.value = current.hostnames.join('\n')
  identityMode.value = current.identity_mode
}
watch(site, load, { immediate: true })

const hostList = computed(() =>
  hostnames.value
    .split(/[\s,]+/)
    .map((host) => host.trim())
    .filter(Boolean),
)
const dirty = computed(
  () =>
    !!site.value &&
    (identityMode.value !== site.value.identity_mode ||
      hostList.value.join('\n') !== site.value.hostnames.join('\n')),
)

async function onSave() {
  await runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId },
        body: {
          project_id: props.projectId,
          hostnames: hostList.value,
          identity_mode: identityMode.value,
        },
      }),
    success: 'Saved collection settings',
  })
}

const ingestUrl = computed(
  () => `${window.location.origin}/api/v1/workspaces/${props.workspaceId}/analytics/events`,
)
const curl = computed(
  () => `curl -X POST '${ingestUrl.value}' \\
  -H 'authorization: Bearer <API key with analytics:write>' \\
  -H 'content-type: application/json' \\
  -d '{"project_id": "${props.projectId}", "events": [{"name": "invoice_paid", "user_id": "user_123", "props": {"amount": 49}}]}'`,
)

const {
  data: goals,
  isPending: goalsPending,
  error: goalsError,
} = useGoals(
  () => props.workspaceId,
  () => props.projectId,
)
const createGoal = useCreateGoal()
const deleteGoal = useDeleteGoal()
const goalColumns = [
  { key: 'name', label: 'Name' },
  { key: 'match', label: 'Matches' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-16 text-right' },
]
const goalKinds: { value: GoalKind; label: string }[] = [
  { value: 'page', label: 'Page path' },
  { value: 'event', label: 'Event name' },
]

const goalPanelOpen = ref(false)
const goalDraft = ref({ name: '', kind: 'event' as GoalKind, match: '' })
function openGoalPanel() {
  goalDraft.value = { name: '', kind: 'event', match: '' }
  createGoal.reset()
  goalPanelOpen.value = true
}
useCreateIntent(openGoalPanel)

async function onCreateGoal() {
  await createGoal.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      project_id: props.projectId,
      name: goalDraft.value.name.trim(),
      kind: goalDraft.value.kind,
      match: goalDraft.value.match.trim(),
    },
  })
  goalPanelOpen.value = false
}

function onDeleteGoal(goal: Goal) {
  return runAction({
    confirm: {
      title: `Delete ${goal.name}?`,
      message: 'Reports stop showing its conversions. Scripts bound to it stop running.',
      confirmLabel: 'Delete goal',
    },
    run: () =>
      deleteGoal.mutateAsync({
        path: { workspace_id: props.workspaceId, goal_id: goal.id },
      }),
    success: `Deleted ${goal.name}`,
  })
}

/** `?goal=<id>` opens that goal's panel, so palette rows can link to it. */
const viewingGoal = computed(() => {
  const id = route.query.goal
  return typeof id === 'string' ? goals.value?.items.find((goal) => goal.id === id) : undefined
})
function viewGoal(goal: Goal | undefined) {
  void router.replace({ query: { ...route.query, goal: goal?.id } })
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="site"
    title="Analytics settings"
    width="narrow"
  >
    <template #default="{ site: current }">
      <Card title="Snippet" description="Add this to the <head> of every page on your site.">
        <div class="flex items-start gap-2">
          <pre
            class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-sm"
          ><code>{{ current.snippet }}</code></pre>
          <CopyButton :text="current.snippet" label="Copy snippet" />
        </div>
        <p class="text-sm text-base-content/70">
          Send custom events and name your users from the page:
        </p>
        <pre
          class="overflow-x-auto rounded-field bg-base-200 p-3 text-sm"
        ><code>{{ TRACKING_EXAMPLE }}</code></pre>
      </Card>

      <form @submit.prevent="onSave">
        <Card
          title="Collection"
          description="Changes reach visitors within 5 minutes, as browsers cache the snippet."
        >
          <Field
            label="Allowed hostnames"
            hint="One per line, exact or *.example.com. Events from other hosts are dropped."
          >
            <textarea
              v-model="hostnames"
              name="analytics-hostnames"
              rows="3"
              class="textarea w-full font-mono text-sm"
              placeholder="example.com"
            ></textarea>
          </Field>
          <p v-if="!hostList.length" role="alert" class="alert text-sm alert-warning">
            Any hostname can send events to this project, including copies of your site.
          </p>
          <Field label="Visitor identity">
            <Select v-model="identityMode" :options="identityModes" label="Visitor identity" />
          </Field>
          <p class="text-sm text-base-content/70">
            Consent is your site's responsibility. Cookieless needs no banner in most places; the
            other modes store an ID in the visitor's browser.
          </p>
          <ErrorAlert :error="update.error.value" />
          <template #footer>
            <SubmitButton :pending="update.asyncStatus.value === 'loading'" :disabled="!dirty">
              Save changes
            </SubmitButton>
          </template>
        </Card>
      </form>

      <Card
        title="Goals"
        description="A page path or event name to count as a conversion, once per person and day."
      >
        <template #actions>
          <button type="button" class="btn gap-1.5 btn-sm" @click="openGoalPanel">
            <Plus class="size-4" aria-hidden="true" />New goal
          </button>
        </template>
        <DataTable
          :columns="goalColumns"
          :rows="goals?.items"
          :row-key="(g) => g.id"
          :loading="goalsPending"
          :error="goalsError"
          label="Goals"
          :empty="{
            icon: Target,
            title: 'No goals yet',
            message: 'Add a goal such as a signup page to see its conversion rate.',
          }"
        >
          <template #cell-name="{ row }">
            <button type="button" class="link font-medium link-hover" @click="viewGoal(row)">
              {{ row.name }}
            </button>
          </template>
          <template #cell-match="{ row }">
            <span class="text-sm"
              >{{ row.kind === 'page' ? 'Page' : 'Event' }}
              <code class="text-sm">{{ row.match }}</code></span
            >
          </template>
          <template #cell-actions="{ row }">
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              :aria-label="`Delete ${row.name}`"
              @click="onDeleteGoal(row)"
            >
              <Trash2 class="size-4" aria-hidden="true" />
            </button>
          </template>
        </DataTable>
      </Card>

      <Card
        title="Server-side events"
        description="Send events from your backend with a workspace API key that has the analytics:write scope."
      >
        <div class="flex items-start gap-2">
          <pre
            class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-sm"
          ><code>{{ curl }}</code></pre>
          <CopyButton :text="curl" label="Copy curl example" />
        </div>
      </Card>

      <FormPanel
        :open="goalPanelOpen"
        title="New goal"
        submit-label="Create goal"
        :pending="createGoal.asyncStatus.value === 'loading'"
        :error="createGoal.error.value"
        @close="goalPanelOpen = false"
        @submit="onCreateGoal"
      >
        <Field label="Name">
          <input
            v-model="goalDraft.name"
            name="goal-name"
            required
            class="input w-full"
            placeholder="Signup"
          />
        </Field>
        <Field label="Matches">
          <Select v-model="goalDraft.kind" :options="goalKinds" label="Goal kind" />
        </Field>
        <Field :label="goalDraft.kind === 'page' ? 'Page path' : 'Event name'">
          <input
            v-model="goalDraft.match"
            name="goal-match"
            required
            class="input w-full"
            :placeholder="goalDraft.kind === 'page' ? '/signup/done' : 'signup_completed'"
          />
        </Field>
      </FormPanel>
      <GoalPanel
        :workspace-id="workspaceId"
        :project-id="projectId"
        :goal="viewingGoal"
        @close="viewGoal(undefined)"
      />
    </template>
  </AnalyticsShell>
</template>
