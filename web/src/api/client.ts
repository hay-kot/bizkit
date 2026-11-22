import { client } from './gen/client.gen'

/**
 * Points the generated client at this origin with the session cookie, and
 * calls `onUnauthenticated` when an API call answers 401.
 */
export function configureClient(onUnauthenticated: () => void | Promise<void>) {
  client.setConfig({ baseUrl: '', credentials: 'same-origin' })
  client.interceptors.response.use((response) => {
    const path = new URL(response.url, window.location.origin).pathname
    if (response.status === 401 && !path.startsWith('/api/auth/')) {
      void onUnauthenticated()
    }
    return response
  })
}
