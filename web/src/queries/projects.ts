import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  projectsCreateMutation,
  projectsDeleteMutation,
  projectsListQuery,
} from '@/api/gen/@pinia/colada.gen'
import type { Project } from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { Project }

/** The server's cap. The sidebar switcher needs every project, not the default first 100. */
const PROJECTS_LIMIT = 500

export function useProjects(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() =>
    projectsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { limit: PROJECTS_LIMIT },
    }),
  )
}

export function useCreateProject() {
  const invalidate = useInvalidate()
  return useMutation({ ...projectsCreateMutation(), onSettled: () => invalidate('projectsList') })
}

export function useDeleteProject() {
  const invalidate = useInvalidate()
  return useMutation({ ...projectsDeleteMutation(), onSettled: () => invalidate('projectsList') })
}
