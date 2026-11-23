import { onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

/**
 * Opens a list page's create panel when the URL asks with `?new=1` (the
 * palette's "New ..." rows link there), then drops the query so a reload or
 * Back doesn't open it again.
 */
export function useCreateIntent(open: () => void): void {
  const route = useRoute()
  const router = useRouter()
  // A leaving page stays mounted through the route transition and sees the
  // next route's query; only the page the intent targets may act on it.
  const path = route.path
  const check = () => {
    if (route.path !== path || route.query.new !== '1') return
    open()
    const query = { ...route.query }
    delete query.new
    void router.replace({ query })
  }
  onMounted(check)
  watch(() => route.query.new, check)
}
