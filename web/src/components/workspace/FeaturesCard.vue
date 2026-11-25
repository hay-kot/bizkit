<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { toast } from '@/lib/toast'
import { useUpdateWorkspaceFeatures, useWorkspaceFeatures } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const { data, error } = useWorkspaceFeatures(() => props.workspaceId)
const update = useUpdateWorkspaceFeatures()

const enabled = ref<string[]>([])
const saved = computed(() =>
  (data.value?.items ?? []).filter((feature) => feature.enabled).map((feature) => feature.name),
)
watch(saved, (value) => (enabled.value = [...value]), { immediate: true })
const changed = computed(
  () =>
    enabled.value.length !== saved.value.length ||
    enabled.value.some((name) => !saved.value.includes(name)),
)

/** Module names are lowercase words, and their sidebar labels the same word capitalized. */
const label = (name: string) => name.charAt(0).toUpperCase() + name.slice(1)

async function onSave() {
  const disabled = (data.value?.items ?? [])
    .map((feature) => feature.name)
    .filter((name) => !enabled.value.includes(name))
  await update.mutateAsync({ path: { workspace_id: props.workspaceId }, body: { disabled } })
  toast('Features saved')
}
</script>

<template>
  <form @submit.prevent="onSave">
    <Card
      title="Features"
      description="Turn features off to hide them from everyone in this workspace. Their API, MCP tools and public endpoints stop answering. Nothing is deleted: turn a feature on again and its data is back."
    >
      <ErrorAlert v-if="error" :error="error" />
      <SkeletonRows v-else-if="!data" :rows="3" />
      <ul v-else class="flex flex-col gap-3" aria-label="Features">
        <li v-for="feature in data.items" :key="feature.name">
          <Checkbox v-model="enabled" :value="feature.name">
            <span class="flex flex-col">
              <span class="font-medium">{{ label(feature.name) }}</span>
              <span class="text-base-content/70">{{ feature.description }}</span>
            </span>
          </Checkbox>
        </li>
      </ul>
      <ErrorAlert :error="update.error.value" />
      <template #footer>
        <SubmitButton :pending="update.asyncStatus.value === 'loading'" :disabled="!changed">
          Save features
        </SubmitButton>
      </template>
    </Card>
  </form>
</template>
