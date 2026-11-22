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

export function useProjects(workspaceId: MaybeRefOrGetter<string>) {
  return useQuery(() => projectsListQuery({ path: { workspace_id: toValue(workspaceId) } }))
}

export function useCreateProject() {
  const invalidate = useInvalidate()
  return useMutation({ ...projectsCreateMutation(), onSettled: () => invalidate('projectsList') })
}

export function useDeleteProject() {
  const invalidate = useInvalidate()
  return useMutation({ ...projectsDeleteMutation(), onSettled: () => invalidate('projectsList') })
}
