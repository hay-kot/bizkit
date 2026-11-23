import { Code, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import ExtensionsSection from '@/components/extensions/ExtensionsSection.vue'
import Extensions from '@/pages/extensions/Extensions.vue'
import Script from '@/pages/extensions/Script.vue'
import { useScripts } from '@/queries/extensions'

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
  entitySections: [{ key: 'extensions', component: ExtensionsSection, adminOnly: true }],
  palette: (cx) => {
    if (!cx.projectId || !cx.isAdmin) return []
    const { data } = useScripts(cx.workspaceId, cx.projectId)
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/extensions`
    return computed(() => [
      {
        id: 'extensions:new',
        title: 'New script',
        group: 'Extensions',
        order: 110,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add', 'extension', 'hook'],
        to: `${base}?new=1`,
      },
      ...(data.value?.items ?? []).map((script) => ({
        id: `extensions.script:${script.id}`,
        title: script.name,
        group: 'Extensions',
        order: 110,
        icon: Code,
        kind: 'script',
        scope: 'goto' as const,
        keywords: ['open', script.hook],
        to: `${base}/${script.id}`,
      })),
    ])
  },
})
