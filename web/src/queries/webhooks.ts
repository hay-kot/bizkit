import { useMutation, useQuery } from '@pinia/colada'
import type { MaybeRefOrGetter } from 'vue'
import { toValue } from 'vue'

import {
  webhooksEndpointsCreateMutation,
  webhooksEndpointsDeleteMutation,
  webhooksEndpointsGetQuery,
  webhooksEndpointsListQuery,
  webhooksEndpointsUpdateMutation,
  webhooksRequestsGetQuery,
  webhooksRequestsListQuery,
  webhooksRequestsReplayMutation,
} from '@/api/gen/@pinia/colada.gen'
import type {
  CapturedRequest,
  Delivery,
  Endpoint,
  Header,
  RequestSummary,
} from '@/api/gen/types.gen'

import { useInvalidate } from './keys'

export type { CapturedRequest, Delivery, Endpoint, Header, RequestSummary }

const ENDPOINT_QUERIES = ['webhooksEndpointsList', 'webhooksEndpointsGet']
const REQUEST_QUERIES = ['webhooksRequestsList', 'webhooksRequestsGet']

/** The most requests the endpoint page lists. */
export const REQUESTS_SHOWN = 100

export function useEndpoints(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    webhooksEndpointsListQuery({
      path: { workspace_id: toValue(workspaceId) },
      query: { project_id: toValue(projectId) },
    }),
  )
}

export function useEndpoint(
  workspaceId: MaybeRefOrGetter<string>,
  endpointId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    webhooksEndpointsGetQuery({
      path: { workspace_id: toValue(workspaceId), endpoint_id: toValue(endpointId) },
    }),
  )
}

export function useCreateEndpoint() {
  const invalidate = useInvalidate()
  return useMutation({
    ...webhooksEndpointsCreateMutation(),
    onSettled: () => invalidate(...ENDPOINT_QUERIES),
  })
}

export function useUpdateEndpoint() {
  const invalidate = useInvalidate()
  return useMutation({
    ...webhooksEndpointsUpdateMutation(),
    onSettled: () => invalidate(...ENDPOINT_QUERIES),
  })
}

export function useDeleteEndpoint() {
  const invalidate = useInvalidate()
  return useMutation({
    ...webhooksEndpointsDeleteMutation(),
    onSettled: () => invalidate(...ENDPOINT_QUERIES),
  })
}

export function useRequests(
  workspaceId: MaybeRefOrGetter<string>,
  endpointId: MaybeRefOrGetter<string>,
) {
  return useQuery(() =>
    webhooksRequestsListQuery({
      path: { workspace_id: toValue(workspaceId), endpoint_id: toValue(endpointId) },
      query: { limit: REQUESTS_SHOWN },
    }),
  )
}

/** One request in full; idle until `requestId` is set. */
export function useCapturedRequest(
  workspaceId: MaybeRefOrGetter<string>,
  endpointId: MaybeRefOrGetter<string>,
  requestId: MaybeRefOrGetter<string | null>,
) {
  return useQuery(() => ({
    ...webhooksRequestsGetQuery({
      path: {
        workspace_id: toValue(workspaceId),
        endpoint_id: toValue(endpointId),
        request_id: toValue(requestId) ?? '',
      },
    }),
    enabled: toValue(requestId) !== null,
  }))
}

export function useReplayRequest() {
  const invalidate = useInvalidate()
  return useMutation({
    ...webhooksRequestsReplayMutation(),
    onSettled: () => invalidate(...REQUEST_QUERIES),
  })
}
