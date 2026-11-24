import type { PageContext } from '@/components/ui/pageHeader'
import { projectPath } from '@/modules'

/**
 * The workspace shell's `PageContext`. Paths come from the IDs alone, so
 * crumbs link to the right pages while the names are still loading; only
 * the labels wait for them.
 */
export function createPageContext(
  workspaceId: () => string,
  names: {
    workspace: () => string | undefined
    project: (projectId: string) => string | undefined
  },
): PageContext {
  return {
    root: (projectId) =>
      projectId
        ? { label: names.project(projectId) ?? '', to: `/w/${workspaceId()}/p/${projectId}` }
        : { label: names.workspace() ?? '', to: `/w/${workspaceId()}` },
    navTo: (item, projectId) =>
      item.section === 'project' && projectId
        ? projectPath(workspaceId(), projectId, item.path)
        : `/w/${workspaceId()}/${item.path}`,
  }
}
