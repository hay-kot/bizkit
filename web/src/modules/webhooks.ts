import { Plus, Webhook } from 'lucide-vue-next'
import { computed } from 'vue'

import { useEndpoints } from '@/queries/webhooks'

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
  palette: (cx) => {
    if (!cx.projectId) return []
    const { data } = useEndpoints(cx.workspaceId, cx.projectId)
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/webhooks`
    return computed(() => [
      {
        id: 'webhooks:new',
        title: 'New endpoint',
        group: 'Webhooks',
        order: 105,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add', 'webhook'],
        to: `${base}?new=1`,
      },
      ...(data.value?.items ?? []).map((endpoint) => ({
        id: `webhooks.endpoint:${endpoint.id}`,
        title: endpoint.name,
        group: 'Webhooks',
        order: 105,
        icon: Webhook,
        kind: 'endpoint',
        scope: 'goto' as const,
        keywords: ['open', 'webhook'],
        to: `${base}/${endpoint.id}`,
      })),
    ])
  },
})
