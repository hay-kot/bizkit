import type { FieldDef, Form } from '@/queries/forms'

const ALTCHA_SCRIPT = 'https://cdn.jsdelivr.net/npm/altcha/dist/altcha.min.js'

function attr(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
}

const SAMPLE_FIELDS: FieldDef[] = [
  { name: 'name', label: 'Name', kind: 'text', required: true },
  { name: 'email', label: 'Email', kind: 'email', required: true },
  { name: 'message', label: 'Message', kind: 'text', required: true },
]

function input(field: FieldDef): string {
  const label = attr(field.label ?? field.name)
  const name = attr(field.name)
  const required = field.required ? ' required' : ''
  if (field.kind === 'choice') {
    const options = (field.options ?? [])
      .map((option) => `      <option>${attr(option)}</option>`)
      .join('\n')
    const multiple = field.multiple ? ' multiple' : ''
    return `  <label>${label}\n    <select name="${name}"${multiple}${required}>\n${options}\n    </select>\n  </label>`
  }
  const type = { text: 'text', email: 'email', url: 'url', number: 'number' }[field.kind]
  return `  <label>${label} <input type="${type}" name="${name}"${required}></label>`
}

/**
 * A plain HTML form that posts to the form's endpoint. Challenge forms add
 * the ALTCHA widget, which must load its challenge on page load so the
 * minimum fill time counts from when the visitor arrived.
 */
export function htmlSnippet(form: Form): string {
  const fields = form.fields.length > 0 ? form.fields : SAMPLE_FIELDS
  const lines = [
    `<form method="post" action="${attr(form.endpoint)}">`,
    ...fields.map(input),
    '  <input type="text" name="_gotcha" tabindex="-1" autocomplete="off" hidden>',
  ]
  if (form.bot_check === 'challenge') {
    lines.push(
      `  <altcha-widget challengeurl="${attr(form.challenge_url)}" auto="onload"></altcha-widget>`,
    )
  }
  lines.push('  <button type="submit">Send</button>', '</form>')
  if (form.bot_check === 'challenge') {
    lines.unshift(`<script async defer src="${ALTCHA_SCRIPT}" type="module"></script>`)
  }
  return lines.join('\n')
}

/**
 * Posting from script: `URLSearchParams` keeps the body urlencoded (a
 * `FormData` body would be multipart, which ingest refuses), and the
 * `Accept` header asks for JSON instead of a redirect.
 */
export function fetchSnippet(form: Form): string {
  return [
    `const form = document.querySelector('form')`,
    `form.addEventListener('submit', async (event) => {`,
    `  event.preventDefault()`,
    `  const response = await fetch(${JSON.stringify(form.endpoint)}, {`,
    `    method: 'POST',`,
    `    headers: { Accept: 'application/json' },`,
    `    body: new URLSearchParams(new FormData(form)),`,
    `  })`,
    `  const result = await response.json()`,
    `  // 201 {"ok": true}, or 422 with result.error.fields`,
    `})`,
  ].join('\n')
}
