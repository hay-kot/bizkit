import { describe, expect, it } from 'vitest'

import { describeStep, draftOf, emptyFunnel, funnelBody } from './funnelDraft'
import { shadeClass } from './retention'

describe('funnel drafts', () => {
  it('start with two steps and trim on save', () => {
    const draft = emptyFunnel()
    expect(draft.steps).toHaveLength(2)
    draft.name = ' Signup '
    draft.steps[1] = { kind: 'event', match: ' signup_completed ' }
    expect(funnelBody(draft)).toEqual({
      name: 'Signup',
      window_days: 7,
      steps: [
        { kind: 'page', match: '/' },
        { kind: 'event', match: 'signup_completed' },
      ],
    })
  })

  it('copy a saved funnel without sharing its steps', () => {
    const funnel = {
      id: 'f',
      project_id: 'p',
      name: 'Signup',
      window_days: 3,
      steps: [{ kind: 'page' as const, match: '/' }],
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    }
    const draft = draftOf(funnel)
    draft.steps[0]!.match = '/pricing'
    expect(funnel.steps[0]!.match).toBe('/')
    expect(draft.windowDays).toBe(3)
  })

  it('describe steps', () => {
    expect(describeStep({ kind: 'page', match: '/pricing' })).toBe('Visit /pricing')
    expect(describeStep({ kind: 'event', match: 'signup' })).toBe('signup')
  })
})

describe('retention shades', () => {
  it('step from faint to full and clamp', () => {
    expect(shadeClass(-1)).toBe('bg-primary/5')
    expect(shadeClass(0)).toBe('bg-primary/5')
    expect(shadeClass(0.5)).toBe('bg-primary/40')
    expect(shadeClass(1)).toBe('bg-primary text-primary-content')
    expect(shadeClass(2)).toBe('bg-primary text-primary-content')
  })
})
