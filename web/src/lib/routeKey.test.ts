import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { routeKey } from './routeKey'

const page = { template: '<div />' }

function router() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      {
        path: '/w/:workspaceId',
        component: page,
        children: [
          { path: 'p/:projectId/forms/:formId', component: page },
          {
            path: 'p/:projectId/analytics',
            component: page,
            children: [
              { path: '', component: page },
              { path: 'funnels/:funnelId', component: page },
            ],
          },
        ],
      },
    ],
  })
}

describe('routeKey', () => {
  it('keys a leaf page by its whole path', async () => {
    const r = router()
    await r.push('/w/a/p/b/forms/c')
    expect(routeKey(r.currentRoute.value, 1)).toBe('/w/a/p/b/forms/c')
  })

  it('keys a layout by its own path, the same for every child', async () => {
    const r = router()
    await r.push('/w/a/p/b/analytics')
    const overview = routeKey(r.currentRoute.value, 1)
    await r.push('/w/a/p/b/analytics/funnels/f1')
    expect(routeKey(r.currentRoute.value, 1)).toBe(overview)
    expect(overview).toBe('/w/a/p/b/analytics')
  })

  it('changes a layout key when its params change', async () => {
    const r = router()
    await r.push('/w/a/p/b/analytics')
    const first = routeKey(r.currentRoute.value, 1)
    await r.push('/w/a/p/other/analytics')
    expect(routeKey(r.currentRoute.value, 1)).not.toBe(first)
  })
})
