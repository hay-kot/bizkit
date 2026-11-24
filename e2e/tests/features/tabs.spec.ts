import type { Page } from '@playwright/test'

import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

/** Tags the header so a remount (which drops the tag) shows. */
async function tagHeader(page: Page) {
  await page.locator('main header h1').evaluate((el) => {
    el.dataset.kept = 'yes'
  })
}

async function openTab(page: Page, nav: string, tab: string, heading: string) {
  await page.getByRole('navigation', { name: nav }).getByRole('link', { name: tab, exact: true }).click()
  await expect(
    page.getByRole('navigation', { name: nav }).getByRole('link', { name: tab, exact: true }),
  ).toHaveAttribute('aria-current', 'page')
  await expect(page.locator('main header h1')).toHaveText(heading)
  await expect(page.locator('main header h1')).toHaveAttribute('data-kept', 'yes')
}

test('analytics tabs swap only the content', async ({ page, world }) => {
  const project = `/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}`
  await page.goto(`${project}/analytics`)
  await expect(page.locator('main header h1')).toHaveText('Analytics')
  await tagHeader(page)

  await openTab(page, 'Analytics pages', 'Funnels', 'Funnels')
  await expect(page).toHaveURL(`${project}/analytics/funnels`)
  await openTab(page, 'Analytics pages', 'People', 'People')
  await openTab(page, 'Analytics pages', 'Settings', 'Analytics settings')

  await page.goBack()
  await expect(page).toHaveURL(`${project}/analytics/people`)
  await expect(page.locator('main header h1')).toHaveText('People')
  await expect(page.locator('main header h1')).toHaveAttribute('data-kept', 'yes')
})

test('mailing tabs keep the header and swap its actions', async ({ page, world }) => {
  const project = `/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}`
  await page.goto(`${project}/mailing`)
  await expect(page.getByRole('button', { name: 'New list' }).first()).toBeVisible()
  await tagHeader(page)

  await openTab(page, 'Mailing pages', 'Broadcasts', 'Mailing')
  await expect(page.locator('main header').getByRole('button', { name: 'New broadcast' })).toBeVisible()
  await expect(page.locator('main header').getByRole('button', { name: 'New list' })).toHaveCount(0)
  await openTab(page, 'Mailing pages', 'Sequences', 'Mailing')
  await expect(page.locator('main header').getByRole('button', { name: 'New sequence' })).toBeVisible()

  // A deep link renders the tab inside the same layout.
  await page.goto(`${project}/mailing/suppressions`)
  await expect(page.locator('main header').getByRole('button', { name: 'Suppress address' })).toBeVisible()
})
