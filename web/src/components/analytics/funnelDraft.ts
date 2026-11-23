import type { Funnel, GoalKind, Step } from '@/queries/analytics'

/** A funnel being edited: name, window and 2 to 8 ordered steps. */
export interface FunnelDraft {
  name: string
  windowDays: number
  steps: { kind: GoalKind; match: string }[]
}

export const MIN_STEPS = 2
export const MAX_STEPS = 8

/** A blank draft with two steps to fill in. */
export function emptyFunnel(): FunnelDraft {
  return {
    name: '',
    windowDays: 7,
    steps: [
      { kind: 'page', match: '/' },
      { kind: 'event', match: '' },
    ],
  }
}

/** A saved funnel as a draft, for its edit panel. */
export function draftOf(funnel: Funnel): FunnelDraft {
  return {
    name: funnel.name,
    windowDays: funnel.window_days,
    steps: funnel.steps.map((step) => ({ ...step })),
  }
}

/** The API body fields of a draft, with matches trimmed. */
export function funnelBody(draft: FunnelDraft): {
  name: string
  window_days: number
  steps: Step[]
} {
  return {
    name: draft.name.trim(),
    window_days: draft.windowDays,
    steps: draft.steps.map((step) => ({ kind: step.kind, match: step.match.trim() })),
  }
}

/** A step as a short phrase, such as "Visit /pricing". */
export function describeStep(step: Step): string {
  return step.kind === 'page' ? `Visit ${step.match}` : step.match
}
