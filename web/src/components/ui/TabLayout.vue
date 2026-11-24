<script setup lang="ts">
import { provide, reactive, ref } from 'vue'

import Page from './Page.vue'
import PageHeader from './PageHeader.vue'
import Tabs, { type Tab } from './Tabs.vue'
import { usePageCrumbs } from './pageHeader'
import { TAB_LAYOUT, type TabHeader } from './tabLayout'

/**
 * The parent route of a feature with tabs. It renders the page, header and
 * tabs (in the header) once, and the current tab's route in a nested `RouterView`, so a tab
 * switch swaps only the content. Each tab's page renders a `TabPage`, which
 * sets the header's title, description and actions. The header takes its
 * icon and breadcrumb from the route's module.
 *
 * `childProps` go to every tab's page, on top of its route params.
 */
defineProps<{
  tabs: Tab[]
  label: string
  childProps?: Record<string, unknown>
}>()

const header = reactive<TabHeader>({ title: '' })
const actions = ref<HTMLElement | null>(null)
const withActions = ref(0)
const leaving = ref(false)
provide(TAB_LAYOUT, { header, actions, withActions, leaving })
// The tabs name the current page, so the breadcrumb stops at the module.
const crumbs = usePageCrumbs(() => '', false)
</script>

<template>
  <Page>
    <PageHeader
      :title="header.title"
      :description="header.description"
      :crumbs="crumbs.length ? crumbs : undefined"
    >
      <template v-if="withActions > 0" #default>
        <div ref="actions" class="flex flex-wrap items-center gap-2" />
      </template>
      <template #tabs><Tabs :tabs="tabs" :label="label" /></template>
    </PageHeader>
    <RouterView v-slot="{ Component, route }">
      <Transition
        mode="out-in"
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="translate-y-1 opacity-0"
        leave-active-class="transition duration-100 ease-in"
        leave-to-class="opacity-0"
        @before-leave="leaving = true"
        @after-leave="leaving = false"
      >
        <component :is="Component" :key="route.path" v-bind="childProps" />
      </Transition>
    </RouterView>
  </Page>
</template>
