/** A file's bytes as standard base64, without the `data:` prefix. */
export async function fileToBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ''
  // Chunks keep String.fromCharCode under the engine's argument limit.
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000))
  }
  return btoa(binary)
}

/** Hands base64 bytes to the browser as a download named `fileName`. */
export function saveBase64(base64: string, fileName: string, type: string): void {
  const binary = atob(base64)
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
  const url = URL.createObjectURL(new Blob([bytes], { type }))
  const link = document.createElement('a')
  link.href = url
  link.download = fileName
  document.body.append(link)
  link.click()
  link.remove()
  // Revoking at once can cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
}

/** `name` cut to at most `max` characters, keeping its extension. */
export function shortFileName(name: string, max = 200): string {
  const chars = [...name]
  if (chars.length <= max) return name
  const dot = name.lastIndexOf('.')
  const extension = dot > 0 ? [...name.slice(dot)] : []
  if (extension.length >= max) return chars.slice(0, max).join('')
  return [...chars.slice(0, max - extension.length), ...extension].join('')
}
