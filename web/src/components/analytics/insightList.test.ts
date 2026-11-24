import { describe, expect, it } from 'vitest'

import { ANY_DASHBOARD, filterInsights, knownDashboardFilter, NO_DASHBOARD } from './insightList'

const insights = [
  { name: 'Sign-ins', dashboards: [{ id: 'growth' }, { id: 'usage' }] },
  { name: 'API key holders', dashboards: [{ id: 'usage' }] },
  { name: 'Churn', dashboards: [] },
]
const names = (items: typeof insights) => items.map((i) => i.name)

describe('insight list', () => {
  it('filters by name, any case, and by dashboard', () => {
    expect(names(filterInsights(insights, ' sign ', ANY_DASHBOARD))).toEqual(['Sign-ins'])
    expect(names(filterInsights(insights, '', 'growth'))).toEqual(['Sign-ins'])
    expect(names(filterInsights(insights, '', NO_DASHBOARD))).toEqual(['Churn'])
    expect(names(filterInsights(insights, 'api', 'growth'))).toEqual([])
  })

  it('resets a dashboard filter whose dashboard is gone', () => {
    expect(knownDashboardFilter('growth', ['growth', 'usage'])).toBe('growth')
    expect(knownDashboardFilter('growth', ['usage'])).toBe(ANY_DASHBOARD)
    expect(knownDashboardFilter(NO_DASHBOARD, [])).toBe(NO_DASHBOARD)
    expect(knownDashboardFilter('growth', undefined)).toBe('growth')
  })
})
