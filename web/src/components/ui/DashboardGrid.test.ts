import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'

import DashboardGrid, { type GridItem, type HandleBindings } from './DashboardGrid.vue'
import DashboardTile from './DashboardTile.vue'
import { dropIndex, moveItem } from './dashboardGrid'
import SegmentedControl from './SegmentedControl.vue'

const items: GridItem[] = [
  { id: 'a', width: 4, label: 'Signups' },
  { id: 'b', width: 8, label: 'Sign-ins' },
  { id: 'c', width: 12, label: 'Keys' },
]

function mountGrid(editing = true, locked = false) {
  return mount(DashboardGrid, {
    props: { items, editing, locked },
    slots: {
      tile: ({ item, handle }: { item: GridItem; handle: HandleBindings }) =>
        h('button', { ...handle, type: 'button' }, item.label),
    },
    attachTo: document.body,
  })
}

describe('moveItem and dropIndex', () => {
  it('moves an entry to an index', () => {
    expect(moveItem(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a'])
    expect(moveItem(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b'])
  })

  it('turns a drop beside a tile into the new index of the dragged one', () => {
    expect(dropIndex(0, { index: 2, after: true })).toBe(2)
    expect(dropIndex(0, { index: 2, after: false })).toBe(1)
    expect(dropIndex(2, { index: 0, after: false })).toBe(0)
    expect(dropIndex(1, { index: 1, after: true })).toBeUndefined()
    expect(dropIndex(1, { index: 2, after: false })).toBeUndefined()
  })
})

describe('DashboardGrid', () => {
  it('spans each tile by its width on wide screens', () => {
    const wrapper = mountGrid(false)
    const cells = wrapper.findAll('[data-index]')
    expect(cells.map((cell) => cell.classes().find((c) => c.startsWith('lg:col-span')))).toEqual([
      'lg:col-span-4',
      'lg:col-span-8',
      'lg:col-span-12',
    ])
    expect(wrapper.classes()).toEqual(expect.arrayContaining(['grid-cols-1', 'lg:grid-cols-12']))
    wrapper.unmount()
  })

  it('moves a tile with the arrow keys and announces where it went', async () => {
    const wrapper = mountGrid()
    const handle = wrapper.get('[data-grid-handle="a"]')
    expect(handle.attributes('aria-label')).toBe('Move Signups')
    await handle.trigger('keydown', { key: 'ArrowRight' })
    expect(wrapper.emitted('move')).toEqual([[0, 1]])
    expect(wrapper.get('[role="status"]').text()).toBe('Signups moved to position 2 of 3')
    await handle.trigger('keydown', { key: 'ArrowLeft' })
    expect(wrapper.emitted('move')).toHaveLength(1)
    await wrapper.get('[data-grid-handle="b"]').trigger('keydown', { key: 'End' })
    expect(wrapper.emitted('move')?.[1]).toEqual([1, 2])
    wrapper.unmount()
  })
})

function placeBox(element: Element, left: number, top: number, width: number, height: number) {
  vi.spyOn(element, 'getBoundingClientRect').mockReturnValue({
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  })
}

/** Lays the three tiles out in one 300px row: 0-100, 100-200, 200-300. */
function layOut(wrapper: ReturnType<typeof mountGrid>) {
  placeBox(wrapper.element, 0, 0, 300, 100)
  for (const cell of wrapper.findAll('[data-index]')) {
    placeBox(cell.element, Number(cell.attributes('data-index')) * 100, 0, 100, 100)
  }
  for (const handle of wrapper.findAll('[data-grid-handle]')) {
    Object.assign(handle.element, { setPointerCapture: () => undefined })
  }
}

describe('DashboardGrid dragging', () => {
  it('moves a tile dropped beside another', async () => {
    const wrapper = mountGrid()
    layOut(wrapper)
    const handle = wrapper.get('[data-grid-handle="a"]')
    await handle.trigger('pointerdown', { button: 0, pointerId: 1 })
    await handle.trigger('pointermove', { clientX: 290, clientY: 50 })
    await handle.trigger('pointerup')
    expect(wrapper.emitted('move')).toEqual([[0, 2]])
    wrapper.unmount()
  })

  it('cancels a drop outside the grid', async () => {
    const wrapper = mountGrid()
    layOut(wrapper)
    const handle = wrapper.get('[data-grid-handle="a"]')
    await handle.trigger('pointerdown', { button: 0, pointerId: 1 })
    await handle.trigger('pointermove', { clientX: 290, clientY: 50 })
    await handle.trigger('pointermove', { clientX: 290, clientY: 400 })
    await handle.trigger('pointerup')
    expect(wrapper.emitted('move')).toBeUndefined()
    wrapper.unmount()
  })

  it('moves nothing while locked', async () => {
    const wrapper = mountGrid(true, true)
    layOut(wrapper)
    const handle = wrapper.get('[data-grid-handle="a"]')
    await handle.trigger('keydown', { key: 'ArrowRight' })
    await handle.trigger('pointerdown', { button: 0, pointerId: 1 })
    await handle.trigger('pointermove', { clientX: 290, clientY: 50 })
    await handle.trigger('pointerup')
    expect(wrapper.emitted('move')).toBeUndefined()
    wrapper.unmount()
  })
})

describe('DashboardTile', () => {
  it('disables the handle, width and Remove while locked', () => {
    const wrapper = mount(DashboardTile, {
      props: {
        title: 'Signups',
        to: '/x',
        editing: true,
        locked: true,
        width: 6,
        handle: { 'aria-label': 'Move Signups' } as unknown as HandleBindings,
      },
      global: { stubs: { RouterLink: true } },
    })
    const buttons = wrapper.findAll('button')
    expect(buttons.length).toBeGreaterThan(2)
    expect(buttons.every((button) => button.attributes('disabled') !== undefined)).toBe(true)
  })
})

describe('SegmentedControl', () => {
  it('is a radio group that arrow keys move through', async () => {
    const options = [
      { value: 'third', label: '1/3' },
      { value: 'half', label: '1/2' },
    ]
    const wrapper = mount(SegmentedControl, {
      props: { label: 'Width', options, modelValue: 'third' },
      attachTo: document.body,
    })
    const radios = wrapper.findAll('[role="radio"]')
    expect(radios.map((radio) => radio.attributes('aria-checked'))).toEqual(['true', 'false'])
    expect(radios.map((radio) => radio.attributes('tabindex'))).toEqual(['0', '-1'])
    await radios[0]?.trigger('keydown', { key: 'ArrowRight' })
    expect(wrapper.emitted('update:modelValue')).toEqual([['half']])
    expect(document.activeElement).toBe(radios[1]?.element)
    await radios[0]?.trigger('click')
    expect(wrapper.emitted('update:modelValue')?.[1]).toEqual(['third'])
    wrapper.unmount()
  })
})

describe('SegmentedControl disabled', () => {
  it('ignores clicks and keys', async () => {
    const wrapper = mount(SegmentedControl, {
      props: {
        label: 'Width',
        options: [
          { value: 'third', label: '1/3' },
          { value: 'half', label: '1/2' },
        ],
        modelValue: 'third',
        disabled: true,
      },
    })
    const radios = wrapper.findAll('[role="radio"]')
    await radios[0]?.trigger('keydown', { key: 'ArrowRight' })
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    expect(radios[1]?.attributes('disabled')).toBeDefined()
  })
})
