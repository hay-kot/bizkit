<script setup lang="ts">
import EntitySections from '@/components/EntitySections.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import { formatDateTime } from '@/lib/format'
import type { Goal } from '@/queries/analytics'

/**
 * A goal's details in a side panel, with the blocks other modules add for
 * goals (the scripts that run when someone converts).
 */
defineProps<{ workspaceId: string; projectId: string; goal: Goal | undefined }>()
defineEmits<{ close: [] }>()
</script>

<template>
  <DetailPanel
    :open="goal !== undefined"
    :title="goal?.name ?? 'Goal'"
    description="A conversion counts once per person and UTC day."
    @close="$emit('close')"
  >
    <template v-if="goal">
      <p class="text-sm">
        {{ goal.kind === 'page' ? 'Pageviews of' : 'Events named' }}
        <code class="font-medium">{{ goal.match }}</code>, created
        {{ formatDateTime(goal.created_at) }}.
      </p>
      <EntitySections
        :workspace-id="workspaceId"
        :project-id="projectId"
        kind="analytics.goal"
        :entity-id="goal.id"
      />
    </template>
  </DetailPanel>
</template>
