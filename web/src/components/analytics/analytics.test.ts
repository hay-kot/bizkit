import { describe, expect, it } from 'vitest'

import { describeStep, draftOf, emptyFunnel, funnelBody } from './funnelDraft'
import { inProgress, meanRow, shadeClass } from './retention'

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

describe('retention weeks', () => {
  // Cohorts start on Mondays; 2026-11-02 is one.
  it('are in progress from their first day through their last', () => {
    expect(inProgress('2026-11-02', 0, '2026-11-01')).toBe(false)
    expect(inProgress('2026-11-02', 0, '2026-11-02')).toBe(true)
    expect(inProgress('2026-11-02', 0, '2026-11-08')).toBe(true)
    expect(inProgress('2026-11-02', 0, '2026-11-09')).toBe(false)
    expect(inProgress('2026-11-02', 1, '2026-11-09')).toBe(true)
    expect(inProgress('2026-11-02', 1, '2026-11-15')).toBe(true)
    expect(inProgress('2026-11-02', 1, '2026-11-16')).toBe(false)
  })

  it('average ended weeks weighted by cohort people', () => {
    const cohorts = [
      { week: '2026-11-02', people: 10, weeks: [1, 0.5, 0.2] },
      { week: '2026-11-09', people: 2, weeks: [1, 0, null] },
    ]
    // On Nov 17 week 0 has ended for both, week 1 only for the first.
    expect(meanRow(cohorts, '2026-11-17', 4)).toEqual([1, 0.5, null, null])
    // On Nov 23 week 1 has ended for both: (5 + 0) / 12.
    expect(meanRow(cohorts, '2026-11-23', 3)).toEqual([1, 5 / 12, 0.2])
  })

  it('give nothing without cohorts', () => {
    expect(meanRow([], '2026-11-17', 2)).toEqual([null, null])
  })
})
