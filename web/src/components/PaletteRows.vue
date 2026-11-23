<script setup lang="ts">
import { computed, toRef } from 'vue'

import { modules, projectNav, projectPath, workspaceNav } from '@/modules'
import type { NavItem } from '@/modules/types'
import { usePaletteRows } from '@/palette/registry'
import type { PaletteContext, PaletteRow } from '@/palette/types'

/**
 * Registers the workspace's palette rows: every nav item the user can see,
 * then each module's `palette` source. Key it by workspace, project and admin
 * flag, so the sources run again with the new context.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string | undefined
  projectName: string
  workspaceName: string
  isAdmin: boolean
}>()

const visible = (items: NavItem[]) => items.filter((item) => !item.adminOnly || props.isAdmin)

usePaletteRows(
  computed((): PaletteRow[] => {
    const projectRows = props.projectId
      ? visible(projectNav).map((item): PaletteRow => ({
          id: `page:project:${item.path}`,
          title: item.label,
          group: props.projectName,
          order: 10,
          icon: item.icon,
          kind: 'page',
          scope: 'goto',
          keywords: ['go to', 'open'],
          to: projectPath(props.workspaceId, props.projectId!, item.path),
        }))
      : []
    const workspaceRows = visible(workspaceNav).map((item): PaletteRow => ({
      id: `page:workspace:${item.path}`,
      title: item.label,
      group: props.workspaceName,
      order: 20,
      icon: item.icon,
      kind: 'page',
      scope: 'goto',
      keywords: ['go to', 'open'],
      to: `/w/${props.workspaceId}/${item.path}`,
    }))
    return [...projectRows, ...workspaceRows]
  }),
)

const cx: PaletteContext = {
  workspaceId: props.workspaceId,
  projectId: props.projectId,
  projectName: toRef(props, 'projectName'),
  isAdmin: props.isAdmin,
}
for (const module of modules) {
  if (module.palette) usePaletteRows(module.palette(cx))
}
</script>

<template><span hidden></span></template>
