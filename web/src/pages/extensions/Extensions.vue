<script setup lang="ts">
import { Code, KeyRound, Plus, Trash2 } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import NewScriptPanel from '@/components/extensions/NewScriptPanel.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import { runsForText } from '@/components/extensions/targets'
import type { ScriptSummary } from '@/queries/extensions'
import {
  useDeleteSecret,
  useHooks,
  usePutSecret,
  useScripts,
  useSecrets,
} from '@/queries/extensions'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const { data: hooks } = useHooks(() => props.workspaceId)
const {
  data: scripts,
  isPending,
  error,
} = useScripts(
  () => props.workspaceId,
  () => props.projectId,
)
const {
  data: secrets,
  isPending: secretsPending,
  error: secretsError,
} = useSecrets(
  () => props.workspaceId,
  () => props.projectId,
)
const putSecret = usePutSecret()
const deleteSecret = useDeleteSecret()

const scriptColumns = [
  { key: 'name', label: 'Name' },
  { key: 'hook', label: 'Hook' },
  { key: 'target', label: 'Runs for' },
  { key: 'updated', label: 'Updated' },
]

const pointsById = computed(() => new Map((hooks.value?.items ?? []).map((h) => [h.id, h])))

function targetKind(script: ScriptSummary) {
  return pointsById.value.get(script.hook)?.target ?? undefined
}

function targetText(script: ScriptSummary): string {
  return runsForText(script.targets, targetKind(script))
}

const ALL = 'all'
const FOR_ALL = 'for-all'
const targetFilter = ref(ALL)
/** The entities scripts name, by ID, with their kind and label. */
const namedTargets = computed(() => {
  const seen = new Map<string, { kind: string | undefined; label: string }>()
  for (const script of scripts.value?.items ?? []) {
    const kind = targetKind(script)
    for (const target of script.targets) {
      seen.set(target.id, {
        kind: kind?.id,
        label: target.label ?? `Deleted ${kind?.noun ?? 'entity'}`,
      })
    }
  }
  return seen
})
const targetOptions = computed(() => [
  { value: ALL, label: 'All scripts' },
  { value: FOR_ALL, label: 'Scripts for every entity' },
  ...[...namedTargets.value].map(([value, { label }]) => ({ value, label })),
])
/** A named entity shows the scripts that run for it: those naming it and those for all of its kind. */
const shownScripts = computed(() =>
  scripts.value?.items.filter((script) => {
    if (targetFilter.value === ALL) return true
    if (script.targets.length === 0) {
      if (targetFilter.value === FOR_ALL) return true
      const kind = targetKind(script)?.id
      return kind !== undefined && namedTargets.value.get(targetFilter.value)?.kind === kind
    }
    return script.targets.some((target) => target.id === targetFilter.value)
  }),
)
const secretColumns = [
  { key: 'name', label: 'Name' },
  { key: 'updated', label: 'Set' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-16 text-right' },
]

const panelOpen = ref(false)

function scriptPath(scriptId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/extensions/${scriptId}`
}

const secretOpen = ref(false)
const secretName = ref('')
const secretValue = ref('')

function openSecret() {
  secretName.value = ''
  secretValue.value = ''
  putSecret.reset()
  secretOpen.value = true
}

async function onPutSecret() {
  await putSecret.mutateAsync({
    path: {
      workspace_id: props.workspaceId,
      project_id: props.projectId,
      name: secretName.value,
    },
    body: { value: secretValue.value },
  })
  secretOpen.value = false
}

function removeSecret(secret: string) {
  return runAction({
    confirm: {
      title: `Delete ${secret}?`,
      message: 'Scripts that read it get undefined from then on.',
      confirmLabel: 'Delete secret',
    },
    run: () =>
      deleteSecret.mutateAsync({
        path: { workspace_id: props.workspaceId, project_id: props.projectId, name: secret },
      }),
    success: 'Secret deleted',
  })
}
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      title="Extensions"
      description="Scripts that run on this project's hook points, such as after each form submission."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="panelOpen = true">
        <Plus class="size-4" aria-hidden="true" />New script
      </button>
    </PageHeader>

    <PageSection title="Scripts">
      <template v-if="(scripts?.items.length ?? 0) > 0" #actions>
        <Select
          v-model="targetFilter"
          :options="targetOptions"
          label="Runs for"
          size="sm"
          name="target"
        />
      </template>
      <DataTable
        :columns="scriptColumns"
        :rows="shownScripts"
        :row-key="(s) => s.id"
        :loading="isPending"
        :error="error"
        label="Scripts"
        :empty="{
          icon: Code,
          title: 'No scripts yet',
          message: 'Attach a script to a hook point to filter spam or post submissions elsewhere.',
        }"
      >
        <template #cell-name="{ row }">
          <RouterLink :to="scriptPath(row.id)" class="link font-medium link-hover">
            {{ row.name }}
          </RouterLink>
          <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
          <span v-if="!row.available" class="ms-2 badge badge-sm badge-warning">Unavailable</span>
        </template>
        <template #cell-hook="{ row }">
          <code class="text-xs">{{ row.hook }}</code>
          <span class="ms-2 badge badge-outline badge-sm">{{ row.sync ? 'sync' : 'async' }}</span>
        </template>
        <template #cell-target="{ row }"
          ><span class="text-sm" :class="{ 'text-base-content/70': row.targets.length === 0 }">{{
            targetText(row)
          }}</span></template
        >
        <template #cell-updated="{ row }"
          ><span class="text-sm">{{ formatDate(row.updated_at) }}</span></template
        >
        <template #empty-action>
          <button type="button" class="btn btn-primary btn-sm" @click="panelOpen = true">
            New script
          </button>
        </template>
      </DataTable>
    </PageSection>

    <PageSection title="Secrets">
      <template #description>
        Async scripts read these as <code>ctx.secrets</code>. Values are never shown again, and run
        logs show <code>[secret:NAME]</code> instead.
      </template>
      <template #actions>
        <button type="button" class="btn gap-1.5 btn-sm" @click="openSecret">
          <Plus class="size-4" aria-hidden="true" />Add secret
        </button>
      </template>
      <DataTable
        :columns="secretColumns"
        :rows="secrets?.items"
        :row-key="(s) => s.name"
        :loading="secretsPending"
        :error="secretsError"
        label="Secrets"
        :empty="{ icon: KeyRound, title: 'No secrets', message: 'Add a webhook URL or API token.' }"
      >
        <template #cell-name="{ row }"
          ><code class="text-sm">{{ row.name }}</code></template
        >
        <template #cell-updated="{ row }"
          ><span class="text-sm">{{ formatDate(row.updated_at) }}</span></template
        >
        <template #cell-actions="{ row }">
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="`Delete ${row.name}`"
            @click="removeSecret(row.name)"
          >
            <Trash2 class="size-4" aria-hidden="true" />
          </button>
        </template>
      </DataTable>
    </PageSection>

    <NewScriptPanel
      :open="panelOpen"
      :workspace-id="workspaceId"
      :project-id="projectId"
      @close="panelOpen = false"
    />

    <FormPanel
      :open="secretOpen"
      title="Add secret"
      submit-label="Save secret"
      :pending="putSecret.asyncStatus.value === 'loading'"
      :error="putSecret.error.value"
      @close="secretOpen = false"
      @submit="onPutSecret"
    >
      <Field label="Name" hint="Uppercase letters, digits and _, starting with a letter.">
        <input
          v-model="secretName"
          name="secret-name"
          required
          pattern="[A-Z][A-Z0-9_]{0,63}"
          class="input w-full font-mono"
          placeholder="DISCORD_WEBHOOK"
        />
      </Field>
      <Field label="Value" hint="Saving an existing name replaces its value.">
        <input
          v-model="secretValue"
          name="secret-value"
          type="password"
          required
          autocomplete="off"
          class="input w-full"
        />
      </Field>
    </FormPanel>
  </Page>
</template>
