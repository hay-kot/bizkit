import type { MaybeRefOrGetter } from 'vue'
import { computed, toValue, watch } from 'vue'
import { useRoute } from 'vue-router'

import type { Project } from '@/queries/projects'
import { useProjects } from '@/queries/projects'

const storageKey = (workspaceId: string) => `bizkit.project.${workspaceId}`

/**
 * The project the sidebar's project section points at. A project in the URL
 * wins, even when it no longer exists (the page then shows its own error);
 * otherwise the last project used in this workspace, otherwise the first.
 */
export function pickProject(
  projects: Project[],
  routeId: string | undefined,
  storedId: string | null,
): Project | undefined {
  if (routeId) return projects.find((p) => p.id === routeId)
  return projects.find((p) => p.id === storedId) ?? projects[0]
}

function readStored(workspaceId: string): string | null {
  try {
    return localStorage.getItem(storageKey(workspaceId))
  } catch {
    // Storage can be blocked (private mode, policies); fall back to the first project.
    return null
  }
}

function store(workspaceId: string, projectId: string) {
  try {
    localStorage.setItem(storageKey(workspaceId), projectId)
  } catch {
    // Remembering the project is a convenience; nothing to do when storage is blocked.
  }
}

export function useCurrentProject(workspaceId: MaybeRefOrGetter<string>) {
  const route = useRoute()
  const { data, isPending, error } = useProjects(workspaceId)
  const projects = computed(() => data.value?.items ?? [])
  const routeId = computed(() =>
    typeof route.params.projectId === 'string' ? route.params.projectId : undefined,
  )
  const project = computed(() =>
    pickProject(projects.value, routeId.value, readStored(toValue(workspaceId))),
  )
  watch(
    routeId,
    (id) => {
      if (id) store(toValue(workspaceId), id)
    },
    { immediate: true },
  )
  return { projects, project, isPending, error }
}
