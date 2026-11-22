import { describe, expect, it } from 'vitest'

import type { Form } from '@/queries/forms'

import { fetchSnippet, htmlSnippet } from './embed'

function form(overrides: Partial<Form> = {}): Form {
  return {
    id: 'f1',
    project_id: 'p1',
    name: 'Contact',
    enabled: true,
    fields: [],
    bot_check: 'honeypot',
    min_fill_ms: 2000,
    redirect_url: null,
    notify_emails: [],
    endpoint: 'https://hub.example.com/m/forms/public/f1',
    challenge_url: 'https://hub.example.com/m/forms/public/f1/challenge',
    inbox_count: 0,
    created_at: '2026-11-22T00:00:00Z',
    updated_at: '2026-11-22T00:00:00Z',
    ...overrides,
  }
}

describe('embed snippets', () => {
  it('posts a plain form with the honeypot', () => {
    const html = htmlSnippet(form())
    expect(html).toContain(
      '<form method="post" action="https://hub.example.com/m/forms/public/f1">',
    )
    expect(html).toContain('name="_gotcha"')
    expect(html).toContain('name="email" required')
    expect(html).not.toContain('altcha')
  })

  it('adds the widget for challenge forms, loading on page load', () => {
    const html = htmlSnippet(form({ bot_check: 'challenge' }))
    expect(html).toContain(
      '<altcha-widget challengeurl="https://hub.example.com/m/forms/public/f1/challenge" auto="onload">',
    )
    expect(html).toMatch(/^<script async defer src="[^"]+altcha@2\.3\.0[^"]+" integrity="sha384-/)
  })

  it('uses the defined fields and escapes them', () => {
    const html = htmlSnippet(
      form({
        fields: [
          { name: 'topic', label: 'A "topic"', kind: 'choice', options: ['<b>'], multiple: true },
        ],
      }),
    )
    expect(html).toContain('<label>A &quot;topic&quot;')
    expect(html).toContain('<select name="topic" multiple>')
    expect(html).toContain('<option>&lt;b&gt;</option>')
  })

  it('fetches with an urlencoded body and asks for JSON', () => {
    const js = fetchSnippet(form())
    expect(js).toContain('new URLSearchParams(new FormData(form))')
    expect(js).toContain("Accept: 'application/json'")
  })
})
