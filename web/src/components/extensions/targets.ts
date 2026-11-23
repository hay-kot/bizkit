import type { HookTargetKind, ScriptTarget } from '@/queries/extensions'

/** What a script runs for, in words: "All forms", "Contact", "Contact, Quote". */
export function runsForText(targets: ScriptTarget[], kind: HookTargetKind | undefined): string {
  if (targets.length === 0) return kind ? `All ${kind.plural}` : 'The project'
  return targets.map((target) => target.label ?? `Deleted ${kind?.noun ?? 'entity'}`).join(', ')
}
