import { FileSignature, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import { useSigningRequests } from '@/queries/signing'

import Request from '@/pages/signing/Request.vue'
import Requests from '@/pages/signing/Requests.vue'
import Sign from '@/pages/signing/Sign.vue'

import { defineWebModule } from './types'

const ORDER = 107

export default defineWebModule({
  name: 'signing',
  projectRoutes: [
    { path: 'signing', component: Requests },
    // The completion mail links the sender here.
    { path: 'signing/:requestId', component: Request },
  ],
  // A signer's mailed link opens here.
  publicRoutes: [{ path: 'sign', component: Sign }],
  nav: [
    { path: 'signing', label: 'Signing', icon: FileSignature, section: 'project', order: ORDER },
  ],
  palette: (cx) => {
    if (!cx.projectId) return []
    const { data } = useSigningRequests(cx.workspaceId, cx.projectId)
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/signing`
    return computed(() => [
      {
        id: 'signing:new',
        title: 'New signature request',
        group: 'Signing',
        order: ORDER,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'send', 'sign', 'pdf', 'contract', 'upload'],
        to: `${base}?new=1`,
      },
      ...(data.value?.items ?? []).map((request) => ({
        id: `signing.request:${request.id}`,
        title: request.title,
        group: 'Signing',
        order: ORDER,
        icon: FileSignature,
        kind: 'request',
        scope: 'goto' as const,
        keywords: ['open', 'signature', request.file_name],
        to: `${base}/${request.id}`,
      })),
    ])
  },
})
