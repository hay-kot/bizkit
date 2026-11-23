import { ChartLine, Filter, Plus, Target } from 'lucide-vue-next'
import { computed } from 'vue'

import Events from '@/pages/analytics/Events.vue'
import Funnel from '@/pages/analytics/Funnel.vue'
import Funnels from '@/pages/analytics/Funnels.vue'
import Overview from '@/pages/analytics/Overview.vue'
import People from '@/pages/analytics/People.vue'
import Person from '@/pages/analytics/Person.vue'
import Retention from '@/pages/analytics/Retention.vue'
import Settings from '@/pages/analytics/Settings.vue'
import { useFunnels, useGoals } from '@/queries/analytics'
import type { PaletteRow } from '@/palette/types'

import { defineWebModule } from './types'

const ORDER = 102

export default defineWebModule({
  name: 'analytics',
  projectRoutes: [
    { path: 'analytics', component: Overview },
    { path: 'analytics/events', component: Events },
    { path: 'analytics/funnels', component: Funnels },
    { path: 'analytics/funnels/:funnelId', component: Funnel },
    { path: 'analytics/people', component: People },
    { path: 'analytics/people/:personId', component: Person },
    { path: 'analytics/retention', component: Retention },
    { path: 'analytics/settings', component: Settings },
  ],
  nav: [
    { path: 'analytics', label: 'Analytics', icon: ChartLine, section: 'project', order: ORDER },
  ],
  palette: (cx) => {
    if (!cx.projectId) return []
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/analytics`
    const { data: funnels } = useFunnels(cx.workspaceId, cx.projectId)
    const { data: goals } = useGoals(cx.workspaceId, cx.projectId)
    const page = (path: string, title: string): PaletteRow => ({
      id: `page:analytics:${path}`,
      title,
      group: 'Analytics',
      order: ORDER,
      icon: ChartLine,
      kind: 'page',
      scope: 'goto',
      keywords: ['go to', 'open', 'analytics'],
      to: `${base}/${path}`,
    })
    return computed((): PaletteRow[] => [
      page('events', 'Events'),
      page('funnels', 'Funnels'),
      page('people', 'People'),
      page('retention', 'Retention'),
      ...(cx.isAdmin ? [page('settings', 'Analytics settings')] : []),
      {
        id: 'analytics:new-funnel',
        title: 'New funnel',
        group: 'Analytics',
        order: ORDER,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add', 'funnel', 'analytics'],
        to: `${base}/funnels?new=1`,
      },
      ...(cx.isAdmin
        ? [
            {
              id: 'analytics:new-goal',
              title: 'New goal',
              group: 'Analytics',
              order: ORDER,
              icon: Plus,
              kind: 'action',
              keywords: ['create', 'add', 'goal', 'conversion', 'analytics'],
              to: `${base}/settings?new=1`,
            },
          ]
        : []),
      ...(funnels.value?.items ?? []).map((funnel) => ({
        id: `analytics.funnel:${funnel.id}`,
        title: funnel.name,
        group: 'Analytics',
        order: ORDER,
        icon: Filter,
        kind: 'funnel',
        scope: 'goto' as const,
        keywords: ['open', 'funnel'],
        to: `${base}/funnels/${funnel.id}`,
      })),
      ...(cx.isAdmin ? (goals.value?.items ?? []) : []).map((goal) => ({
        id: `analytics.goal:${goal.id}`,
        title: goal.name,
        group: 'Analytics',
        order: ORDER,
        icon: Target,
        kind: 'goal',
        scope: 'goto' as const,
        keywords: ['open', 'goal', 'conversion'],
        to: `${base}/settings?goal=${goal.id}`,
      })),
    ])
  },
})
