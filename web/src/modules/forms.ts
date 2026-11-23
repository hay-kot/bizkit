import { Inbox, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import { useForms } from '@/queries/forms'

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
  palette: (cx) => {
    if (!cx.projectId) return []
    const { data } = useForms(cx.workspaceId, cx.projectId)
    const base = `/w/${cx.workspaceId}/p/${cx.projectId}/forms`
    return computed(() => [
      {
        id: 'forms:new',
        title: 'New form',
        group: 'Forms',
        order: 100,
        icon: Plus,
        kind: 'action',
        keywords: ['create', 'add'],
        to: `${base}?new=1`,
      },
      ...(data.value?.items ?? []).map((form) => ({
        id: `forms.form:${form.id}`,
        title: form.name,
        group: 'Forms',
        order: 100,
        icon: Inbox,
        kind: 'form',
        scope: 'goto' as const,
        keywords: ['open'],
        to: `${base}/${form.id}`,
      })),
    ])
  },
})
