import { Inbox } from 'lucide-vue-next'

import Form from '@/pages/forms/Form.vue'
import Forms from '@/pages/forms/Forms.vue'

import { defineWebModule } from './types'

export default defineWebModule({
  name: 'forms',
  projectRoutes: [
    { path: 'forms', component: Forms },
    { path: 'forms/:formId', component: Form },
  ],
  nav: [{ path: 'forms', label: 'Forms', icon: Inbox, section: 'project', order: 100 }],
})
