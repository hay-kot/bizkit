import { Mail, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import MailingSection from '@/components/mailing/MailingSection.vue'
import Broadcast from '@/pages/mailing/Broadcast.vue'
import Broadcasts from '@/pages/mailing/Broadcasts.vue'
import List from '@/pages/mailing/List.vue'
import Lists from '@/pages/mailing/Lists.vue'
import { useMailingLists } from '@/queries/mailing'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'mailing',
  projectRoutes: [
    { path: 'mailing', component: Lists },
    { path: 'mailing/lists/:listId', component: List },
    { path: 'mailing/broadcasts', component: Broadcasts },
    { path: 'mailing/broadcasts/:broadcastId', component: Broadcast },
  ],
  nav: [{ path: 'mailing', label: 'Mailing', icon: Mail, section: 'project', order: 103 }],
  entitySections: [{ key: 'mailing', component: MailingSection }],
  palette: (cx) => {
    if (!cx.projectId) return []
    const { data } = useMailingLists(cx.workspaceId, cx.projectId)
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/mailing`
    return computed(() => [
      {
        id: 'mailing:new-broadcast',
        title: 'New broadcast',
        group: 'Mailing',
        order: 103,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'write', 'email', 'newsletter', 'send'],
        to: `${base}/broadcasts?new=1`,
      },
      ...(cx.isAdmin
        ? [
            {
              id: 'mailing:new-list',
              title: 'New list',
              group: 'Mailing',
              order: 103,
              icon: Plus,
              kind: 'action',
              keywords: ['create', 'add', 'mailing list', 'newsletter'],
              to: `${base}?new=1`,
            },
          ]
        : []),
      ...(data.value?.items ?? []).map((list) => ({
        id: `mailing.list:${list.id}`,
        title: list.name,
        group: 'Mailing',
        order: 103,
        icon: Mail,
        kind: 'list',
        scope: 'goto' as const,
        keywords: ['open', 'mailing list', 'contacts', 'subscribers'],
        to: `${base}/lists/${list.id}`,
      })),
    ])
  },
})
