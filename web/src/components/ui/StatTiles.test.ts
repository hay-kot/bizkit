import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import StatTiles from './StatTiles.vue'

const text = (s: string) => s.replace(/\s+/g, ' ').trim()

describe('StatTiles', () => {
  it('shows values and deltas against the previous period', () => {
    const wrapper = mount(StatTiles, {
      props: {
        tiles: [
          { label: 'Visitors', value: 1284, previous: 1000 },
          { label: 'Events', value: 97, previous: 100 },
          { label: 'Rate', value: 0.037, format: 'percent' },
          { label: 'Goals', value: 5, previous: 0 },
          { label: 'Idle', value: 0, previous: 0 },
        ],
      },
    })
    const tiles = wrapper.findAll('dl > div').map((tile) => ({
      label: tile.get('dt').text(),
      shown: [tile.get('.text-2xl'), ...tile.findAll('span[aria-hidden="true"]')].map((d) =>
        d.text(),
      ),
      spoken: tile.find('.sr-only').exists() ? tile.get('.sr-only').text() : undefined,
      tone: tile.find('.badge-success').exists()
        ? 'success'
        : tile.find('.badge-error').exists()
          ? 'error'
          : undefined,
    }))
    expect(tiles).toEqual([
      {
        label: 'Visitors',
        shown: ['1,284', '28.4%'],
        spoken: 'up 28.4% from the previous period',
        tone: 'success',
      },
      {
        label: 'Events',
        shown: ['97', '3%'],
        spoken: 'down 3% from the previous period',
        tone: 'error',
      },
      { label: 'Rate', shown: ['3.7%'], spoken: undefined, tone: undefined },
      {
        label: 'Goals',
        shown: ['5', 'new'],
        spoken: 'new since the previous period',
        tone: 'success',
      },
      {
        label: 'Idle',
        shown: ['0', '0%'],
        spoken: 'no change from the previous period',
        tone: undefined,
      },
    ])
  })

  it('says the previous value and draws a sparkline when given a series', () => {
    const wrapper = mount(StatTiles, {
      props: {
        tiles: [
          { label: 'Visitors', value: 76, previous: 51, series: [3, 5, 4], partialFrom: 2 },
          { label: 'Rate', value: 0.2, previous: 0.25, format: 'percent' },
          { label: 'Events', value: 9 },
        ],
      },
    })
    const tiles = wrapper.findAll('dl > div')
    expect(text(tiles[0]!.text())).toContain('vs. 51 prior')
    expect(text(tiles[1]!.text())).toContain('vs. 25% prior')
    expect(tiles[2]!.text()).not.toContain('prior')
    expect(tiles[0]!.find('[data-part="partial"]').exists()).toBe(true)
    expect(tiles[1]!.find('[data-part="area"]').exists()).toBe(false)
  })

  it('shows skeletons while loading', () => {
    const wrapper = mount(StatTiles, {
      props: { tiles: [{ label: 'Visitors', value: 0 }], loading: true },
    })
    expect(wrapper.get('dl').attributes('aria-busy')).toBe('true')
    expect(wrapper.find('.animate-pulse').exists()).toBe(true)
    expect(text(wrapper.text())).toBe('Visitors')
  })
})
