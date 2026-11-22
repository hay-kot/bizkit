import { Inbox } from 'lucide-vue-next'

import Form from '@/pages/forms/Form.vue'
import Forms from '@/pages/forms/Forms.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'forms',
  workspaceRoutes: [
    { path: 'forms', component: Forms },
    { path: 'forms/:formId', component: Form, props: true },
  ],
  nav: [{ path: 'forms', label: 'Forms', icon: Inbox, order: 100 }],
})
