import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import { analyticsInsightsQuery } from '@/api/gen'
import {
  analyticsDashboardsCreateMutation,
  analyticsDashboardsDeleteMutation,
  analyticsDashboardsGetQuery,
  analyticsDashboardsListQuery,
  analyticsDashboardsUpdateMutation,
  analyticsEventsQueryQuery,
  analyticsEventsRecentQuery,
  analyticsFunnelsCreateMutation,
  analyticsFunnelsDeleteMutation,
  analyticsFunnelsDropoffsQuery,
  analyticsFunnelsGetQuery,
  analyticsFunnelsListQuery,
  analyticsFunnelsResultsQuery,
  analyticsFunnelsUpdateMutation,
  analyticsGoalsCreateMutation,
  analyticsGoalsDeleteMutation,
  analyticsGoalsListQuery,
  analyticsInsightsCreateMutation,
  analyticsInsightsDeleteMutation,
  analyticsInsightsGetQuery,
  analyticsInsightsListQuery,
  analyticsInsightsUpdateMutation,
  analyticsPeopleGetQuery,
  analyticsPeopleListQuery,
  analyticsRetentionQuery,
  analyticsSchemaEventsQuery,
  analyticsSchemaKeysQuery,
  analyticsSchemaValuesQuery,
  analyticsSiteCreateMutation,
  analyticsSiteGetQuery,
  analyticsSiteUpdateMutation,
  analyticsStatsBreakdownQuery,
  analyticsStatsSummaryQuery,
} from '@/api/gen/@pinia/colada.gen'
import type {
  AnalyticsSchemaEventsData,
  AnalyticsSchemaKeysData,
  AnalyticsSchemaValuesData,
  BreakdownRow,
  BreakdownSource,
  Cohort,
  Dashboard,
  Dimension,
  Dropoff,
  DropoffPage,
  EventItem,
  EventName,
  FilterOp,
  Funnel,
  Goal,
  GoalKind,
  IdentityMode,
  Insight,
  InsightDisplay,
  InsightFilter,
  InsightKind,
  InsightQuery,
  InsightRange,
  InsightResult,
  Measure,
  PersonSummary,
  SchemaEvent,
  SchemaKey,
  SchemaSource,
  SchemaValue,
  SchemaValueType,
  Site,
  Step,
  StepResult,
  Summary,
} from '@/api/gen/types.gen'
import { errorCode } from '@/lib/errors'
import type { DayRange } from '@/lib/range'

import { useInvalidate } from './keys'

export type {
  BreakdownRow,
  BreakdownSource,
  Cohort,
  Dashboard,
  Dimension,
  Dropoff,
  DropoffPage,
  EventItem,
  EventName,
  FilterOp,
  Funnel,
  Goal,
  GoalKind,
  IdentityMode,
  Insight,
  InsightDisplay,
  InsightFilter,
  InsightKind,
  InsightQuery,
  InsightRange,
  InsightResult,
  Measure,
  PersonSummary,
  SchemaEvent,
  SchemaKey,
  SchemaSource,
  SchemaValue,
  SchemaValueType,
  Site,
  Step,
  StepResult,
  Summary,
}

/** The site; the query fails with `not_found` while analytics is off. */
export function useSite(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsSiteGetQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

/** True when `error` says the project has no analytics site. */
export function isSiteMissing(error: unknown): boolean {
  return errorCode(error) === 'not_found'
}

export function useCreateSite() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsSiteCreateMutation(),
    onSettled: () => invalidate('analyticsSiteGet'),
  })
}

export function useUpdateSite() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsSiteUpdateMutation(),
    onSettled: () => invalidate('analyticsSiteGet'),
  })
}

export function useGoals(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsGoalsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

const GOAL_QUERIES = ['analyticsGoalsList', 'analyticsStatsBreakdown', 'analyticsStatsSummary']

export function useCreateGoal() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsGoalsCreateMutation(),
    onSettled: () => invalidate(...GOAL_QUERIES),
  })
}

export function useDeleteGoal() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsGoalsDeleteMutation(),
    onSettled: () => invalidate(...GOAL_QUERIES),
  })
}

export function useSummary(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  compare: MaybeRefOrGetter<boolean>,
) {
  return useQuery(() =>
    analyticsStatsSummaryQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: {
        project_id: toValue(projectId),
        ...toValue(range),
        compare: toValue(compare),
      },
    }),
  )
}

export function useBreakdown(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  dimension: MaybeRefOrGetter<Dimension>,
) {
  return useQuery(() =>
    analyticsStatsBreakdownQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: {
        project_id: toValue(projectId),
        ...toValue(range),
        dimension: toValue(dimension),
        limit: 100,
      },
    }),
  )
}

/** A name and `key=value` property filters for the event explorer. */
export interface EventFilter {
  name: string | null
  props: string[]
}

export function useEventStats(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  filter: MaybeRefOrGetter<EventFilter>,
) {
  return useQuery(() =>
    analyticsEventsQueryQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId), ...toValue(range), ...toValue(filter) },
    }),
  )
}

/** The most recent events the explorer lists. */
export const RECENT_SHOWN = 50

export function useRecentEvents(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  filter: MaybeRefOrGetter<EventFilter>,
) {
  return useQuery(() =>
    analyticsEventsRecentQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: {
        project_id: toValue(projectId),
        ...toValue(range),
        ...toValue(filter),
        limit: RECENT_SHOWN,
      },
    }),
  )
}

const FUNNEL_QUERIES = [
  'analyticsFunnelsList',
  'analyticsFunnelsGet',
  'analyticsFunnelsResults',
  'analyticsFunnelsDropoffs',
]

export function useFunnels(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsFunnelsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useFunnel(
  workspaceId: MaybeRefOrGetter<string>,
  funnelId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsFunnelsGetQuery({
      path: { workspace_id: toValue(workspaceId), funnel_id: toValue(funnelId) },
    }),
  )
}

export function useFunnelResults(
  workspaceId: MaybeRefOrGetter<string>,
  funnelId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
) {
  return useQuery(() =>
    analyticsFunnelsResultsQuery({
      path: { workspace_id: toValue(workspaceId), funnel_id: toValue(funnelId) },
      query: toValue(range),
    }),
  )
}

/** Who dropped off before `step`; idle until a step is picked. */
export function useFunnelDropoffs(
  workspaceId: MaybeRefOrGetter<string>,
  funnelId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  step: MaybeRefOrGetter<number | undefined>,
) {
  return useQuery(() => {
    const { from, to } = toValue(range)
    return {
      ...analyticsFunnelsDropoffsQuery({
        path: { workspace_id: toValue(workspaceId), funnel_id: toValue(funnelId) },
        query: { from, to, step: toValue(step) ?? 2 },
      }),
      enabled: toValue(step) !== undefined,
    }
  })
}

export function useCreateFunnel() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsFunnelsCreateMutation(),
    onSettled: () => invalidate(...FUNNEL_QUERIES),
  })
}

export function useUpdateFunnel() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsFunnelsUpdateMutation(),
    onSettled: () => invalidate(...FUNNEL_QUERIES),
  })
}

export function useDeleteFunnel() {
  const invalidate = useInvalidate()
  // Only the list: the deleted funnel's page is still mounted, and
  // refetching it would flash a 404 before the page leaves.
  return useMutation({
    ...analyticsFunnelsDeleteMutation(),
    onSettled: () => invalidate('analyticsFunnelsList'),
  })
}

/** The most people the People page lists. */
export const PEOPLE_SHOWN = 100

export function usePeople(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  search: MaybeRefOrGetter<string>,
) {
  return useQuery(() => {
    const q = toValue(search).trim()
    return analyticsPeopleListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId), q: q === '' ? null : q, limit: PEOPLE_SHOWN },
    })
  })
}

/** The most timeline events the person page shows. */
export const TIMELINE_SHOWN = 200

export function usePerson(
  workspaceId: MaybeRefOrGetter<string>,
  personId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsPeopleGetQuery({
      path: { workspace_id: toValue(workspaceId), person_id: toValue(personId) },
      query: { limit: TIMELINE_SHOWN },
    }),
  )
}

export function useRetention(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  returnEvent: MaybeRefOrGetter<string | null>,
) {
  return useQuery(() =>
    analyticsRetentionQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: {
        project_id: toValue(projectId),
        ...toValue(range),
        return_event: toValue(returnEvent),
      },
    }),
  )
}

/**
 * An insight query's numbers. The operation is a POST (the query is a JSON
 * body), so this builds its own cached query instead of a generated one.
 * Idle while `enabled` is false.
 */
export function useInsightResult(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  query: MaybeRefOrGetter<InsightQuery>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery(() => {
    const path = { workspace_id: toValue(workspaceId) }
    const { from, to } = toValue(range)
    const body = { project_id: toValue(projectId), from, to, query: toValue(query) }
    return {
      key: [{ _id: 'analyticsInsightsQuery', path, body: JSON.parse(JSON.stringify(body)) }],
      query: async ({ signal }) => {
        const { data } = await analyticsInsightsQuery({ path, body, signal, throwOnError: true })
        return data
      },
      enabled: toValue(enabled),
    }
  })
}

const INSIGHT_QUERIES = [
  'analyticsInsightsList',
  'analyticsInsightsGet',
  'analyticsDashboardsList',
  'analyticsDashboardsGet',
]

export function useInsights(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsInsightsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useInsight(
  workspaceId: MaybeRefOrGetter<string>,
  insightId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsInsightsGetQuery({
      path: { workspace_id: toValue(workspaceId), insight_id: toValue(insightId) },
    }),
  )
}

export function useCreateInsight() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsInsightsCreateMutation(),
    onSettled: () => invalidate(...INSIGHT_QUERIES),
  })
}

export function useUpdateInsight() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsInsightsUpdateMutation(),
    onSettled: () => invalidate(...INSIGHT_QUERIES),
  })
}

export function useDeleteInsight() {
  const invalidate = useInvalidate()
  // Not the insight itself: its page is still mounted, and refetching it
  // would flash a 404 before the page leaves.
  return useMutation({
    ...analyticsInsightsDeleteMutation(),
    onSettled: () =>
      invalidate('analyticsInsightsList', 'analyticsDashboardsList', 'analyticsDashboardsGet'),
  })
}

export function useDashboards(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsDashboardsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useDashboard(
  workspaceId: MaybeRefOrGetter<string>,
  dashboardId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    analyticsDashboardsGetQuery({
      path: { workspace_id: toValue(workspaceId), dashboard_id: toValue(dashboardId) },
    }),
  )
}

export function useCreateDashboard() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsDashboardsCreateMutation(),
    onSettled: () => invalidate(...INSIGHT_QUERIES),
  })
}

export function useUpdateDashboard() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsDashboardsUpdateMutation(),
    onSettled: () => invalidate(...INSIGHT_QUERIES),
  })
}

export function useDeleteDashboard() {
  const invalidate = useInvalidate()
  return useMutation({
    ...analyticsDashboardsDeleteMutation(),
    onSettled: () =>
      invalidate('analyticsDashboardsList', 'analyticsInsightsList', 'analyticsInsightsGet'),
  })
}

/**
 * Schema lookups feed pickers while someone types, and a project's names and
 * keys change slowly, so a minute-old answer is fine and saves a scan.
 */
export const SCHEMA_STALE_MS = 60_000

type SchemaParams<T extends { query: object }> = Omit<T['query'], 'project_id'>

/** Event names the project sent, pageview included, most sent first. */
export function useSchemaEvents(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  params: MaybeRefOrGetter<SchemaParams<AnalyticsSchemaEventsData>> = {},
) {
  return useQuery(() => ({
    ...analyticsSchemaEventsQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId), ...toValue(params) },
    }),
    staleTime: SCHEMA_STALE_MS,
  }))
}

/** Event property or person trait keys, with the types their values take. */
export function useSchemaKeys(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  params: MaybeRefOrGetter<SchemaParams<AnalyticsSchemaKeysData>>,
) {
  return useQuery(() => ({
    ...analyticsSchemaKeysQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId), ...toValue(params) },
    }),
    staleTime: SCHEMA_STALE_MS,
  }))
}

/** The top values of one property or trait key, in text form. Waits for a key. */
export function useSchemaValues(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  params: MaybeRefOrGetter<SchemaParams<AnalyticsSchemaValuesData>>,
) {
  return useQuery(() => ({
    ...analyticsSchemaValuesQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId), ...toValue(params) },
    }),
    staleTime: SCHEMA_STALE_MS,
    enabled: toValue(params).key !== '',
  }))
}

/** Event names as picker options: page views first, then custom events. */
export function eventOptions(events: SchemaEvent[]): {
  value: string
  count: number
  group: string
}[] {
  const pages = events.filter((e) => e.name === 'pageview')
  const custom = events.filter((e) => e.name !== 'pageview')
  return [
    ...pages.map((e) => ({ value: e.name, count: e.events, group: 'Page views' })),
    ...custom.map((e) => ({ value: e.name, count: e.events, group: 'Custom events' })),
  ]
}
