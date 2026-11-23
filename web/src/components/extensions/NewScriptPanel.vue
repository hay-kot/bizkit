<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { useCreateScript, useHooks } from '@/queries/extensions'

import TargetPicker from './TargetPicker.vue'

/**
 * Creates a script and opens it. With a `target`, only the hook points of
 * its kind are offered and the script starts out running for that entity
 * alone; the picker can widen it.
 */
const props = defineProps<{
  open: boolean
  workspaceId: string
  projectId: string
  target?: { kind: string; id: string }
}>()
const emit = defineEmits<{ close: [] }>()
const router = useRouter()
const { data: hooks } = useHooks(() => props.workspaceId)
const create = useCreateScript()

const points = computed(() =>
  (hooks.value?.items ?? []).filter(
    (hook) => !props.target || hook.target?.id === props.target.kind,
  ),
)
const hookOptions = computed(() =>
  points.value.map((hook) => ({
    value: hook.id,
    label: hook.id,
    description: `${hook.kind === 'sync' ? 'In the request' : 'After the event'}: ${hook.summary}`,
  })),
)

const STARTERS: Record<string, string> = {
  sync: `// Return nothing to accept, or { action: 'reject', errors: { field: ['message'] } },
// or { action: 'spam', reason: 'why' }.
export default function (input, ctx) {
  ctx.log('checking', input.form.name)
}
`,
  async: `// Runs after the event. ctx.secrets holds this project's secrets; ctx.fetch reaches public URLs.
export default async function (input, ctx) {
  ctx.log('got', JSON.stringify(input))
}
`,
}

const name = ref('')
const hook = ref('')
const targetIds = ref<string[]>([])
const point = computed(() => points.value.find((h) => h.id === hook.value))

watch(
  () => props.open,
  (open) => {
    if (!open) return
    name.value = ''
    hook.value = hookOptions.value[0]?.value ?? ''
    targetIds.value = props.target ? [props.target.id] : []
    create.reset()
  },
)

async function onCreate() {
  const script = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      name: name.value,
      hook: hook.value,
      project_id: props.projectId,
      target_ids: point.value?.target ? targetIds.value : [],
      source: STARTERS[point.value?.kind ?? 'async'] ?? '',
    },
  })
  emit('close')
  await router.push(`/w/${props.workspaceId}/p/${props.projectId}/extensions/${script.id}`)
}
</script>

<template>
  <FormPanel
    :open="open"
    title="New script"
    submit-label="Create script"
    :pending="create.asyncStatus.value === 'loading'"
    :error="create.error.value"
    @close="emit('close')"
    @submit="onCreate"
  >
    <Field label="Name">
      <input
        v-model="name"
        name="script-name"
        required
        class="input w-full"
        placeholder="Post to Discord"
      />
    </Field>
    <Select v-model="hook" :options="hookOptions" label="Hook point" name="hook" />
    <TargetPicker
      v-if="point?.target"
      v-model="targetIds"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :kind="point.target"
    />
  </FormPanel>
</template>
