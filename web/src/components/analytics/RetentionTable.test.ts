import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import RetentionTable from './RetentionTable.vue'

const cohorts = [
  { week: '2026-11-02', people: 10, weeks: [1, 0.5, 0.2, null] },
  { week: '2026-11-09', people: 2, weeks: [1, 0, null, null] },
]

function render(props: Partial<InstanceType<typeof RetentionTable>['$props']> = {}) {
  return mount(RetentionTable, {
    props: { cohorts, today: '2026-11-17', loading: false, error: undefined, ...props },
  })
}

describe('RetentionTable', () => {
  it('puts the all-cohorts mean first, over ended weeks only', () => {
    const rows = render().findAll('tbody tr')
    expect(rows).toHaveLength(3)
    expect(rows[0]!.text()).toContain('All cohorts')
    expect(rows[0]!.text()).toContain('12')
    expect(rows[0]!.text()).toContain('50%')
    expect(rows[0]!.text()).not.toContain('20%')
    expect(rows[1]!.text()).toContain('Nov 2 to Nov 8')
  })

  it('dashes the weeks still running', () => {
    const running = render().findAll('[data-running]')
    expect(running.map((cell) => cell.attributes('aria-label'))).toEqual([
      '20%, in progress',
      '0%, in progress',
    ])
    expect(running[0]!.classes()).toContain('border-dashed')
  })

  it('keeps the empty state without cohorts', () => {
    const wrapper = render({ cohorts: [] })
    expect(wrapper.text()).toContain('No cohorts in this range')
    expect(wrapper.text()).not.toContain('All cohorts')
  })
})
