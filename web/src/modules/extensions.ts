import { Code } from 'lucide-vue-next'

import Extensions from '@/pages/extensions/Extensions.vue'
import Script from '@/pages/extensions/Script.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'extensions',
  projectRoutes: [
    { path: 'extensions', component: Extensions },
    { path: 'extensions/:scriptId', component: Script },
  ],
  nav: [
    {
      path: 'extensions',
      label: 'Extensions',
      icon: Code,
      section: 'project',
      order: 110,
      adminOnly: true,
    },
  ],
})
