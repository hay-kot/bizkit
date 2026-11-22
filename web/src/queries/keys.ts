import { useQueryCache } from '@pinia/colada'

/**
 * Generated query keys are `[{ _id, path, query, ... }]`, so invalidation
 * matches on the operation `_id` rather than a key prefix.
 */
export function useInvalidate() {
  const cache = useQueryCache()
  return (...ids: string[]) =>
    cache.invalidateQueries({
      predicate: (entry) => {
        const head = entry.key[0]
        return (
          typeof head === 'object' &&
          head !== null &&
          ids.includes(String((head as { _id?: unknown })._id))
        )
      },
    })
}
