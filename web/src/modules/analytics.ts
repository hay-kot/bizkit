import { ChartLine, Filter, LayoutDashboard, Lightbulb, Plus, Target } from 'lucide-vue-next'
import { computed } from 'vue'

import Dashboard from '@/pages/analytics/Dashboard.vue'
import Dashboards from '@/pages/analytics/Dashboards.vue'
import Events from '@/pages/analytics/Events.vue'
import Funnel from '@/pages/analytics/Funnel.vue'
import Funnels from '@/pages/analytics/Funnels.vue'
import Insight from '@/pages/analytics/Insight.vue'
import Insights from '@/pages/analytics/Insights.vue'
import NewInsight from '@/pages/analytics/NewInsight.vue'
import Overview from '@/pages/analytics/Overview.vue'
import People from '@/pages/analytics/People.vue'
import Person from '@/pages/analytics/Person.vue'
import Retention from '@/pages/analytics/Retention.vue'
import Settings from '@/pages/analytics/Settings.vue'
import { useDashboards, useFunnels, useGoals, useInsights } from '@/queries/analytics'
import type { PaletteRow } from '@/palette/types'

import { defineWebModule } from './types'

const ORDER = 102

export default defineWebModule({
  name: 'analytics',
  projectRoutes: [
    { path: 'analytics', component: Overview },
    { path: 'analytics/events', component: Events },
    { path: 'analytics/insights', component: Insights },
    { path: 'analytics/insights/new', component: NewInsight },
    { path: 'analytics/insights/:insightId', component: Insight },
    { path: 'analytics/dashboards', component: Dashboards },
    { path: 'analytics/dashboards/:dashboardId', component: Dashboard },
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
    const { data: insights } = useInsights(cx.workspaceId, cx.projectId)
    const { data: dashboards } = useDashboards(cx.workspaceId, cx.projectId)
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
      page('insights', 'Insights'),
      page('dashboards', 'Dashboards'),
      page('funnels', 'Funnels'),
      page('people', 'People'),
      page('retention', 'Retention'),
      ...(cx.isAdmin ? [page('settings', 'Analytics settings')] : []),
      {
        id: 'analytics:new-insight',
        title: 'New insight',
        group: 'Analytics',
        order: ORDER,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add', 'insight', 'query', 'analytics'],
        to: `${base}/insights/new`,
      },
      {
        id: 'analytics:new-dashboard',
        title: 'New dashboard',
        group: 'Analytics',
        order: ORDER,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add', 'dashboard', 'analytics'],
        to: `${base}/dashboards?new=1`,
      },
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
      ...(insights.value?.items ?? []).map((insight) => ({
        id: `analytics.insight:${insight.id}`,
        title: insight.name,
        group: 'Analytics',
        order: ORDER,
        icon: Lightbulb,
        kind: 'insight',
        scope: 'goto' as const,
        keywords: ['open', 'insight'],
        to: `${base}/insights/${insight.id}`,
      })),
      ...(dashboards.value?.items ?? []).map((dashboard) => ({
        id: `analytics.dashboard:${dashboard.id}`,
        title: dashboard.name,
        group: 'Analytics',
        order: ORDER,
        icon: LayoutDashboard,
        kind: 'dashboard',
        scope: 'goto' as const,
        keywords: ['open', 'dashboard'],
        to: `${base}/dashboards/${dashboard.id}`,
      })),
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
