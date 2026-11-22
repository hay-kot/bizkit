import { FolderKanban, KeyRound, Mail, Settings, Users } from 'lucide-vue-next'

import ApiKeys from '@/pages/workspace/ApiKeys.vue'
import Invitations from '@/pages/workspace/Invitations.vue'
import Members from '@/pages/workspace/Members.vue'
import Projects from '@/pages/workspace/Projects.vue'
import WorkspaceSettings from '@/pages/workspace/Settings.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'core',
  workspaceRoutes: [
    { path: 'projects', component: Projects },
    { path: 'members', component: Members },
    { path: 'invitations', component: Invitations },
    { path: 'api-keys', component: ApiKeys },
    { path: 'settings', component: WorkspaceSettings },
  ],
  nav: [
    { path: 'projects', label: 'Projects', icon: FolderKanban, order: 10 },
    { path: 'members', label: 'Members', icon: Users, order: 20 },
    { path: 'invitations', label: 'Invitations', icon: Mail, order: 30, adminOnly: true },
    { path: 'api-keys', label: 'API keys', icon: KeyRound, order: 40, adminOnly: true },
    { path: 'settings', label: 'Settings', icon: Settings, order: 90, adminOnly: true },
  ],
})
