<script setup lang="ts">
import { provide, reactive, ref } from 'vue'

import Page from './Page.vue'
import PageHeader from './PageHeader.vue'
import Tabs, { type Tab } from './Tabs.vue'
import { TAB_LAYOUT, type TabHeader } from './tabLayout'

/**
 * The parent route of a feature with tabs. It renders the page, header and
 * tabs once, and the current tab's route in a nested `RouterView`, so a tab
 * switch swaps only the content. Each tab's page renders a `TabPage`, which
 * sets the header's title, description, width and actions.
 *
 * `childProps` go to every tab's page, on top of its route params.
 */
defineProps<{
  section?: string
  tabs: Tab[]
  label: string
  childProps?: Record<string, unknown>
}>()

const header = reactive<TabHeader>({ title: '', width: 'wide' })
const actions = ref<HTMLElement | null>(null)
const withActions = ref(0)
const leaving = ref(false)
provide(TAB_LAYOUT, { header, actions, withActions, leaving })
</script>

<template>
  <Page :width="header.width">
    <PageHeader :section="section" :title="header.title" :description="header.description">
      <template v-if="withActions > 0" #default>
        <div ref="actions" class="flex flex-wrap items-center gap-2" />
      </template>
    </PageHeader>
    <Tabs :tabs="tabs" :label="label" />
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
