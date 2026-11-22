import { KeyRound, Settings, Users } from 'lucide-vue-next'

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
})
