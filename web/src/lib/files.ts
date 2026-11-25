/** `name` cut to at most `max` characters, keeping its extension. */
export function shortFileName(name: string, max = 200): string {
  const chars = [...name]
  if (chars.length <= max) return name
  const dot = name.lastIndexOf('.')
  const extension = dot > 0 ? [...name.slice(dot)] : []
  if (extension.length >= max) return chars.slice(0, max).join('')
  return [...chars.slice(0, max - extension.length), ...extension].join('')
}

/** `file` under a name the API takes: at most 200 characters, extension kept. */
export function withShortName(file: File): File {
  const name = shortFileName(file.name)
  return name === file.name ? file : new File([file], name, { type: file.type })
}
