import Sign from '@/pages/signing/Sign.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'signing',
  // A signer's mailed link opens here; the sender pages come with field placement.
  publicRoutes: [{ path: 'sign', component: Sign }],
  nav: [],
})
