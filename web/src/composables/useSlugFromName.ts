import { ref, watch } from 'vue'

import { slugify } from '@/lib/slug'

/**
 * A name and a slug that follows it until the user edits the slug by hand.
 * Bind `onSlugInput` to the slug input's `@input`.
 */
export function useSlugFromName() {
  const name = ref('')
  const slug = ref('')
  const edited = ref(false)
  watch(name, (value) => {
    if (!edited.value) slug.value = slugify(value)
  })
  return {
    name,
    slug,
    onSlugInput: () => (edited.value = true),
    reset() {
      name.value = ''
      slug.value = ''
      edited.value = false
    },
  }
}
