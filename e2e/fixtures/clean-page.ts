import AxeBuilder from '@axe-core/playwright'
import { expect, type Page } from '@playwright/test'

const WIDTHS = [1440, 390]

/** Collects uncaught page errors; call before navigating. */
export function trackPageErrors(page: Page): Error[] {
  const errors: Error[] = []
  page.on('pageerror', (error) => errors.push(error))
  return errors
}

/**
 * Checks the current page at desktop and phone widths: no horizontal
 * overflow, no uncaught errors, and no WCAG A/AA violations.
 */
export async function expectCleanPage(page: Page, errors: Error[]) {
  const original = page.viewportSize()
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 })
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)
    expect(overflow, `horizontal overflow at ${width}px`).toBeLessThanOrEqual(0)
    const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    const violations = results.violations.map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(' ')).join(', ')})`)
    expect(violations, `axe violations at ${width}px`).toEqual([])
  }
  if (original) await page.setViewportSize(original)
  expect(errors.map((e) => e.message), 'uncaught page errors').toEqual([])
}
