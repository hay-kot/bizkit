import { FolderKanban, KeyRound, LayoutGrid, LogOut, Plus, Settings, Users } from 'lucide-vue-next'
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import type { PaletteRow } from '@/palette/types'
import { useSignOut } from '@/queries/auth'
import { useProjects } from '@/queries/projects'

import ApiKeys from '@/pages/workspace/ApiKeys.vue'
import Projects from '@/pages/workspace/Projects.vue'
import WorkspaceSettings from '@/pages/workspace/Settings.vue'
import Team from '@/pages/workspace/Team.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'core',
  workspaceRoutes: [
    { path: 'projects', component: Projects },
    { path: 'team', component: Team },
    { path: 'api-keys', component: ApiKeys },
    { path: 'settings', component: WorkspaceSettings },
  ],
  nav: [
    { path: 'team', label: 'Team', icon: Users, section: 'workspace', order: 10 },
    {
      path: 'api-keys',
      label: 'API keys',
      icon: KeyRound,
      section: 'workspace',
      order: 20,
      adminOnly: true,
    },
    {
      path: 'settings',
      label: 'Settings',
      icon: Settings,
      section: 'workspace',
      order: 90,
      adminOnly: true,
    },
  ],
  palette: (cx) => {
    const router = useRouter()
    const { data: projects } = useProjects(cx.workspaceId)
    const { mutateAsync: signOut } = useSignOut()
    return computed((): PaletteRow[] => [
      ...(projects.value?.items ?? []).map((project): PaletteRow => ({
        id: `projects.project:${project.id}`,
        title: project.name,
        group: 'Projects',
        order: 30,
        icon: FolderKanban,
        kind: project.id === cx.projectId ? 'current project' : 'project',
        scope: 'goto',
        keywords: ['switch', 'open'],
        to: `/w/${cx.workspaceId}/p/${project.id}`,
      })),
      {
        id: 'projects:new',
        title: 'New project',
        group: 'Projects',
        order: 30,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add'],
        to: `/w/${cx.workspaceId}/projects?new=1`,
      },
      {
        id: 'workspaces:all',
        title: 'All workspaces',
        group: 'Account',
        order: 900,
        icon: LayoutGrid,
        kind: 'page',
        scope: 'goto',
        keywords: ['switch', 'workspace', 'home'],
        to: '/',
      },
      {
        id: 'auth:sign-out',
        title: 'Sign out',
        group: 'Account',
        order: 900,
        icon: LogOut,
        kind: 'action',
        keywords: ['log out', 'logout'],
        run: async () => {
          await signOut({})
          await router.replace('/sign-in')
        },
      },
    ])
  },
})
