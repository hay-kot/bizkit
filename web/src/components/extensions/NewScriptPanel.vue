<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { useCreateScript, useHooks } from '@/queries/extensions'

/**
 * Creates a script and opens it. With a `target`, the script binds to that
 * entity and only the hook points of its kind are offered; without one, it
 * is project-wide.
 */
const props = defineProps<{
  open: boolean
  workspaceId: string
  projectId: string
  target?: { kind: string; id: string; label: string }
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

watch(
  () => props.open,
  (open) => {
    if (!open) return
    name.value = ''
    hook.value = hookOptions.value[0]?.value ?? ''
    create.reset()
  },
)

async function onCreate() {
  const point = points.value.find((h) => h.id === hook.value)
  const script = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      name: name.value,
      hook: hook.value,
      project_id: props.projectId,
      target_id: props.target?.id,
      source: STARTERS[point?.kind ?? 'async'] ?? '',
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
    <p v-if="target" class="text-sm text-base-content/80">
      Runs only for <span class="font-medium">{{ target.label }}</span>.
    </p>
  </FormPanel>
</template>
