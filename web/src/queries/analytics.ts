import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  analyticsEventsQueryQuery,
  analyticsEventsRecentQuery,
  analyticsFunnelsCreateMutation,
  analyticsFunnelsDeleteMutation,
  analyticsFunnelsGetQuery,
  analyticsFunnelsListQuery,
  analyticsFunnelsResultsQuery,
  analyticsFunnelsUpdateMutation,
  analyticsGoalsCreateMutation,
  analyticsGoalsDeleteMutation,
  analyticsGoalsListQuery,
  analyticsPeopleGetQuery,
  analyticsPeopleListQuery,
  analyticsRetentionQuery,
  analyticsSiteCreateMutation,
  analyticsSiteGetQuery,
  analyticsSiteUpdateMutation,
  analyticsStatsBreakdownQuery,
  analyticsStatsSummaryQuery,
} from '@/api/gen/@pinia/colada.gen'
import type {
  BreakdownRow,
  Cohort,
  Dimension,
  EventItem,
  EventName,
  Funnel,
  Goal,
  GoalKind,
  IdentityMode,
  PersonSummary,
  Site,
  Step,
  StepResult,
} from '@/api/gen/types.gen'
import { errorCode } from '@/lib/errors'
import type { DayRange } from '@/lib/range'

import { useInvalidate } from './keys'

export type {
  BreakdownRow,
  Cohort,
  Dimension,
  EventItem,
  EventName,
  Funnel,
  Goal,
  GoalKind,
  IdentityMode,
  PersonSummary,
  Site,
  Step,
  StepResult,
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

const FUNNEL_QUERIES = ['analyticsFunnelsList', 'analyticsFunnelsGet', 'analyticsFunnelsResults']

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
