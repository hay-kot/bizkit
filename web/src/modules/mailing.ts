import { ListOrdered, Mail, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import MailingSection from '@/components/mailing/MailingSection.vue'
import Broadcast from '@/pages/mailing/Broadcast.vue'
import Broadcasts from '@/pages/mailing/Broadcasts.vue'
import List from '@/pages/mailing/List.vue'
import Lists from '@/pages/mailing/Lists.vue'
import Sequence from '@/pages/mailing/Sequence.vue'
import Sequences from '@/pages/mailing/Sequences.vue'
import { useMailingLists, useSequences } from '@/queries/mailing'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'mailing',
  projectRoutes: [
    { path: 'mailing', component: Lists },
    { path: 'mailing/lists/:listId', component: List },
    { path: 'mailing/broadcasts', component: Broadcasts },
    { path: 'mailing/broadcasts/:broadcastId', component: Broadcast },
    { path: 'mailing/sequences', component: Sequences },
    { path: 'mailing/sequences/:sequenceId', component: Sequence },
  ],
  nav: [{ path: 'mailing', label: 'Mailing', icon: Mail, section: 'project', order: 103 }],
  entitySections: [{ key: 'mailing', component: MailingSection }],
  palette: (cx) => {
    if (!cx.projectId) return []
    const { data } = useMailingLists(cx.workspaceId, cx.projectId)
    const { data: sequences } = useSequences(cx.workspaceId, cx.projectId)
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
      {
        id: 'mailing:new-sequence',
        title: 'New sequence',
        group: 'Mailing',
        order: 103,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'drip', 'welcome series', 'automation', 'email'],
        to: `${base}/sequences?new=1`,
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
      ...(sequences.value?.items ?? []).map((sequence) => ({
        id: `mailing.sequence:${sequence.id}`,
        title: sequence.name,
        group: 'Mailing',
        order: 103,
        icon: ListOrdered,
        kind: 'sequence',
        scope: 'goto' as const,
        keywords: ['open', 'drip', 'welcome series', 'steps'],
        to: `${base}/sequences/${sequence.id}`,
      })),
    ])
  },
})
