<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import { MailX, Settings, Trash2, UserPlus, Users } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import {
  type AttributeRow,
  attributesToRows,
  describeAttributes,
  rowsProblem,
  rowsToAttributes,
} from '@/components/mailing/attributes'
import AttributesEditor from '@/components/mailing/AttributesEditor.vue'
import StatusBadge from '@/components/mailing/StatusBadge.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { runAction } from '@/lib/action'
import { formatDate, formatNumber } from '@/lib/format'
import {
  type Contact,
  type ContactStatus,
  ROWS_SHOWN,
  useAddContact,
  useContacts,
  useDeleteContact,
  useDeleteMailingList,
  useMailingForms,
  useMailingList,
  useUnsubscribeContact,
  useUpdateContact,
  useUpdateMailingList,
} from '@/queries/mailing'

const props = defineProps<{
  workspaceId: string
  projectId: string
  listId: string
}>()
const router = useRouter()
const isAdmin = useIsAdmin(() => props.workspaceId)
const { data: list, error: listError } = useMailingList(
  () => props.workspaceId,
  () => props.listId,
)
watch(list, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/mailing/lists/${loaded.id}`)
  }
})
const { data: forms } = useMailingForms(
  () => props.workspaceId,
  () => props.projectId,
)
const status = ref<'' | ContactStatus>('')
const search = ref('')
const query = refDebounced(search, 250)
const {
  data: contacts,
  isPending,
  error,
} = useContacts(
  () => props.workspaceId,
  () => props.listId,
  () => ({ status: status.value === '' ? undefined : status.value, q: query.value }),
)
const statusOptions: { value: '' | ContactStatus; label: string }[] = [
  { value: '', label: 'Every status' },
  { value: 'subscribed', label: 'Subscribed' },
  { value: 'pending', label: 'Pending' },
  { value: 'unsubscribed', label: 'Unsubscribed' },
]
const columns = [
  { key: 'email', label: 'Email' },
  { key: 'name', label: 'Name' },
  { key: 'status', label: 'Status' },
  { key: 'tags', label: 'Tags' },
  { key: 'attributes', label: 'Attributes' },
  { key: 'added', label: 'Added' },
  { key: 'actions', label: 'Actions', hidden: true, interactive: true, class: 'w-24 text-right' },
]

const formName = (id: string) => forms.value?.items.find((f) => f.id === id)?.name ?? id
const facts = computed(() => {
  const current = list.value
  if (!current) return []
  const counts = current.counts
  return [
    { key: 'subscribed', label: 'Subscribed', value: formatNumber(counts?.subscribed ?? 0) },
    { key: 'pending', label: 'Pending', value: formatNumber(counts?.pending ?? 0) },
    { key: 'unsubscribed', label: 'Unsubscribed', value: formatNumber(counts?.unsubscribed ?? 0) },
    { key: 'consent', label: 'Double opt-in', value: current.double_opt_in ? 'On' : 'Off' },
    {
      key: 'forms',
      label: 'Forms',
      value: current.form_ids.length ? current.form_ids.map(formName).join(', ') : 'None',
    },
    { key: 'archive', label: 'Public archive' },
  ]
})

const addContact = useAddContact()
const unsubscribe = useUnsubscribeContact()
const deleteContact = useDeleteContact()
const updateList = useUpdateMailingList()
const deleteList = useDeleteMailingList()

const addOpen = ref(false)
const draft = ref({ email: '', name: '', tags: '', attributes: [] as AttributeRow[] })

const splitTags = (text: string) =>
  text
    .split(',')
    .map((t) => t.trim())
    .filter((t) => t !== '')

function openAdd() {
  draft.value = { email: '', name: '', tags: '', attributes: [] }
  addContact.reset()
  addOpen.value = true
}

async function onAdd() {
  const d = draft.value
  if (rowsProblem(d.attributes)) return
  await addContact.mutateAsync({
    path: { workspace_id: props.workspaceId, list_id: props.listId },
    body: {
      email: d.email,
      name: d.name.trim() === '' ? null : d.name.trim(),
      tags: splitTags(d.tags),
      attributes: rowsToAttributes(d.attributes),
    },
  })
  addOpen.value = false
}

const updateContact = useUpdateContact()
const editing = ref<Contact | null>(null)
const edit = ref({ name: '', tags: '', attributes: [] as AttributeRow[] })

function openEdit(contact: Contact) {
  edit.value = {
    name: contact.name ?? '',
    tags: contact.tags.join(', '),
    attributes: attributesToRows(contact.attributes),
  }
  updateContact.reset()
  editing.value = contact
}

async function onEdit() {
  const contact = editing.value
  if (!contact) return
  const e = edit.value
  if (rowsProblem(e.attributes)) return
  await updateContact.mutateAsync({
    path: { workspace_id: props.workspaceId, list_id: props.listId, contact_id: contact.id },
    body: {
      name: e.name.trim() === '' ? null : e.name.trim(),
      tags: splitTags(e.tags),
      attributes: rowsToAttributes(e.attributes),
    },
  })
  editing.value = null
}

function onUnsubscribe(contact: Contact) {
  return runAction({
    confirm: {
      title: `Unsubscribe ${contact.email}?`,
      message: 'They get no more mail from this list, and nothing can subscribe them again.',
      confirmLabel: 'Unsubscribe',
    },
    run: () =>
      unsubscribe.mutateAsync({
        path: { workspace_id: props.workspaceId, list_id: props.listId, contact_id: contact.id },
      }),
    success: `Unsubscribed ${contact.email}`,
  })
}

function onDeleteContact(contact: Contact) {
  return runAction({
    confirm: {
      title: `Delete ${contact.email}?`,
      message: 'This removes the address from the list. A form can add it again.',
      confirmLabel: 'Delete contact',
    },
    run: () =>
      deleteContact.mutateAsync({
        path: { workspace_id: props.workspaceId, list_id: props.listId, contact_id: contact.id },
      }),
    success: `Deleted ${contact.email}`,
  })
}

const settingsOpen = ref(false)
const settings = ref({
  name: '',
  description: '',
  doubleOptIn: true,
  footer: '',
  formIds: [] as string[],
  archive: false,
})

function openSettings() {
  const current = list.value
  if (!current) return
  settings.value = {
    name: current.name,
    description: current.description ?? '',
    doubleOptIn: current.double_opt_in,
    footer: current.footer ?? '',
    formIds: [...current.form_ids],
    archive: current.archive,
  }
  updateList.reset()
  settingsOpen.value = true
}

async function onSaveSettings() {
  const s = settings.value
  await updateList.mutateAsync({
    path: { workspace_id: props.workspaceId, list_id: props.listId },
    body: {
      name: s.name,
      description: s.description.trim() === '' ? null : s.description.trim(),
      double_opt_in: s.doubleOptIn,
      footer: s.footer.trim() === '' ? null : s.footer.trim(),
      form_ids: s.formIds,
      archive: s.archive,
    },
  })
  settingsOpen.value = false
}

function onDeleteList() {
  const name = list.value?.name ?? 'this list'
  return runAction({
    confirm: {
      title: `Delete ${name}?`,
      message: 'This removes the list, its contacts, its broadcasts and their send log.',
      confirmLabel: 'Delete list',
    },
    run: async () => {
      await deleteList.mutateAsync({
        path: { workspace_id: props.workspaceId, list_id: props.listId },
      })
      return true
    },
    success: `Deleted ${name}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(`/w/${props.workspaceId}/p/${props.projectId}/mailing`)
  })
}

const actions = computed(() => [
  { label: 'Add contact', icon: UserPlus, run: openAdd },
  ...(isAdmin.value ? [{ label: 'Settings', icon: Settings, run: openSettings }] : []),
])
</script>

<template>
  <Page>
    <PageHeader
      :title="list?.name ?? ''"
      :description="list?.description ?? 'Contacts of this list, newest first.'"
    >
      <DetailActions :actions="actions">
        <template v-if="isAdmin" #menu>
          <MenuItem danger @select="onDeleteList">
            <Trash2 class="size-4" aria-hidden="true" />Delete list
          </MenuItem>
        </template>
      </DetailActions>
    </PageHeader>

    <ErrorAlert v-if="listError" :error="listError" />

    <DescriptionList v-if="list" :items="facts">
      <template #archive>
        <a
          v-if="list?.archive"
          :href="list.archive_url"
          target="_blank"
          rel="noopener"
          class="link link-primary"
          >Open</a
        ><span v-else>Off</span>
      </template>
    </DescriptionList>

    <PageSection title="Contacts">
      <template #actions>
        <input
          v-model="search"
          type="search"
          name="contact-search"
          aria-label="Search contacts"
          placeholder="Search"
          class="input h-9 w-36 sm:w-48"
        />
        <div class="w-36 sm:w-40">
          <Select v-model="status" :options="statusOptions" label="Status" size="toolbar" />
        </div>
      </template>
      <DataTable
        :columns="columns"
        :rows="contacts?.items"
        :row-key="(c) => c.id"
        :loading="isPending"
        :error="error"
        label="Contacts"
        :empty="{
          icon: Users,
          title: 'No contacts',
          message: 'Link a form in Settings, or add an address by hand.',
        }"
        @row-click="openEdit"
      >
        <template #cell-email="{ row }"
          ><span class="text-sm font-medium">{{ row.email }}</span></template
        >
        <template #cell-name="{ row }"
          ><span class="text-sm">{{ row.name ?? '—' }}</span></template
        >
        <template #cell-status="{ row }"><StatusBadge :status="row.status" /></template>
        <template #cell-tags="{ row }">
          <span v-for="tag in row.tags" :key="tag" class="me-1 badge badge-ghost badge-sm">{{
            tag
          }}</span>
        </template>
        <template #cell-attributes="{ row }"
          ><span class="text-sm text-base-content/80">{{
            describeAttributes(row.attributes)
          }}</span></template
        >
        <template #cell-added="{ row }"
          ><span class="text-sm whitespace-nowrap">{{ formatDate(row.created_at) }}</span></template
        >
        <template #cell-actions="{ row }">
          <div v-if="row.status !== 'unsubscribed'" class="flex justify-end gap-1">
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              :aria-label="`Unsubscribe ${row.email}`"
              @click="onUnsubscribe(row)"
            >
              <MailX class="size-4" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              :aria-label="`Delete ${row.email}`"
              @click="onDeleteContact(row)"
            >
              <Trash2 class="size-4" aria-hidden="true" />
            </button>
          </div>
        </template>
      </DataTable>
      <p v-if="contacts?.next_before" class="text-sm text-base-content/80">
        Showing the newest {{ ROWS_SHOWN }}. Search to find others.
      </p>
    </PageSection>

    <FormPanel
      :open="addOpen"
      title="Add contact"
      :description="
        list?.double_opt_in
          ? 'They get a mail to confirm, and nothing else until they do.'
          : 'They are subscribed at once. Only add people who asked for this mail.'
      "
      submit-label="Add contact"
      :pending="addContact.asyncStatus.value === 'loading'"
      :error="addContact.error.value"
      @close="addOpen = false"
      @submit="onAdd"
    >
      <Field label="Email">
        <input
          v-model="draft.email"
          type="email"
          name="contact-email"
          required
          class="input w-full"
        />
      </Field>
      <Field label="Name" hint="Optional. Fills {{ name }} in mail.">
        <input v-model="draft.name" name="contact-name" class="input w-full" />
      </Field>
      <Field label="Tags" hint="Optional, comma separated. Broadcasts can target tags.">
        <input
          v-model="draft.tags"
          name="contact-tags"
          class="input w-full"
          placeholder="vip, beta"
        />
      </Field>
      <AttributesEditor
        v-model="draft.attributes"
        name="contact-attribute"
        label="Attributes (optional; fill {{ attr.key }} in mail)"
      />
    </FormPanel>

    <FormPanel
      :open="editing !== null"
      :title="editing?.email ?? ''"
      description="Name, tags and attributes. Saving replaces the attributes with these rows."
      submit-label="Save contact"
      :pending="updateContact.asyncStatus.value === 'loading'"
      :error="updateContact.error.value"
      @close="editing = null"
      @submit="onEdit"
    >
      <Field label="Name" hint="Optional. Fills {{ name }} in mail.">
        <input v-model="edit.name" name="edit-contact-name" class="input w-full" />
      </Field>
      <Field label="Tags" hint="Optional, comma separated.">
        <input v-model="edit.tags" name="edit-contact-tags" class="input w-full" />
      </Field>
      <AttributesEditor
        v-model="edit.attributes"
        name="edit-contact-attribute"
        label="Attributes (fill {{ attr.key }} in mail)"
      />
    </FormPanel>

    <FormPanel
      :open="settingsOpen"
      title="List settings"
      submit-label="Save settings"
      :pending="updateList.asyncStatus.value === 'loading'"
      :error="updateList.error.value"
      @close="settingsOpen = false"
      @submit="onSaveSettings"
    >
      <Field label="Name">
        <input v-model="settings.name" name="settings-name" required class="input w-full" />
      </Field>
      <Field label="Description">
        <input v-model="settings.description" name="settings-description" class="input w-full" />
      </Field>
      <Checkbox
        v-model="settings.doubleOptIn"
        label="Addresses added by hand or the API confirm by mail"
      />
      <Checkbox
        v-model="settings.archive"
        label="Publish a public archive and RSS feed of the broadcasts marked for it"
      />
      <p v-if="settings.archive" class="text-sm break-all text-base-content/70">
        Anyone with this link can read them: {{ list?.archive_url }}
      </p>
      <Field label="Footer" hint="Shown at the bottom of every mail, such as your postal address.">
        <textarea
          v-model="settings.footer"
          name="settings-footer"
          rows="3"
          class="textarea w-full"
        ></textarea>
      </Field>
      <fieldset class="flex flex-col gap-2">
        <legend class="mb-1 text-sm font-medium">Forms that add their submitters</legend>
        <p v-if="!forms?.items.length" class="text-sm text-base-content/70">
          This project has no forms.
        </p>
        <Checkbox
          v-for="form in forms?.items ?? []"
          :key="form.id"
          v-model="settings.formIds"
          :value="form.id"
          :label="form.name"
        />
        <p class="text-sm text-base-content/70">Submitters always confirm by mail first.</p>
      </fieldset>
    </FormPanel>
  </Page>
</template>
