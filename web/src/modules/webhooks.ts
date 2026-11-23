import { Webhook } from 'lucide-vue-next'

import Endpoint from '@/pages/webhooks/Endpoint.vue'
import Webhooks from '@/pages/webhooks/Webhooks.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'webhooks',
  projectRoutes: [
    { path: 'webhooks', component: Webhooks },
    { path: 'webhooks/:endpointId', component: Endpoint },
  ],
  nav: [{ path: 'webhooks', label: 'Webhooks', icon: Webhook, section: 'project', order: 105 }],
})
