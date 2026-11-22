import { describe, expect, it } from 'vitest'

import type { Project } from '@/queries/projects'

import { pickProject } from './useCurrentProject'

const project = (id: string) => ({ id, name: id, slug: id }) as Project
const projects = [project('a'), project('b')]

describe('pickProject', () => {
  it('follows the URL', () => {
    expect(pickProject(projects, 'b', 'a')?.id).toBe('b')
  })

  it('does not guess when the URL names a missing project', () => {
    expect(pickProject(projects, 'gone', 'a')).toBeUndefined()
  })

  it('falls back to the stored project, then the first', () => {
    expect(pickProject(projects, undefined, 'b')?.id).toBe('b')
    expect(pickProject(projects, undefined, 'gone')?.id).toBe('a')
    expect(pickProject(projects, undefined, null)?.id).toBe('a')
    expect(pickProject([], undefined, 'a')).toBeUndefined()
  })
})
