import { Inbox, Users } from 'lucide-vue-next'
import { describe, expect, it } from 'vitest'

import type { NavItem } from '@/modules/types'

import { createPageContext } from './pageContext'

const forms: NavItem = {
  path: 'forms',
  label: 'Forms',
  icon: Inbox,
  section: 'project',
  order: 100,
}
const team: NavItem = { path: 'team', label: 'Team', icon: Users, section: 'workspace', order: 10 }

describe('createPageContext', () => {
  it('links project pages from the route IDs while the names load', () => {
    const context = createPageContext(() => 'ws', {
      workspace: () => undefined,
      project: () => undefined,
    })
    expect(context.root('p1')).toEqual({ label: '', to: '/w/ws/p/p1' })
    expect(context.navTo(forms, 'p1')).toBe('/w/ws/p/p1/forms')
    expect(context.root(undefined)).toEqual({ label: '', to: '/w/ws' })
    expect(context.navTo(team, undefined)).toBe('/w/ws/team')
  })

  it('labels the root with the loaded names', () => {
    const context = createPageContext(() => 'ws', {
      workspace: () => 'Acme',
      project: (id) => (id === 'p1' ? 'Website' : undefined),
    })
    expect(context.root('p1').label).toBe('Website')
    expect(context.root(undefined).label).toBe('Acme')
  })
})
