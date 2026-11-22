import './styles/app.css'

import { PiniaColada } from '@pinia/colada'
import { createPinia } from 'pinia'
import { createApp } from 'vue'

import { configureClient } from './api/client'
import App from './App.vue'
import { router } from './router'

// Until the first navigation resolves, currentRoute is the start location and
// has no meta, so the check waits for the router.
configureClient(async () => {
  await router.isReady()
  const current = router.currentRoute.value
  if (!current.meta.public) {
    await router.replace({ path: '/sign-in', query: { next: current.fullPath } })
  }
})

createApp(App)
  .use(createPinia())
  .use(PiniaColada, { queryOptions: { refetchOnWindowFocus: false } })
  .use(router)
  .mount('#app')
