<script setup lang="ts">
import { UserPlus } from 'lucide-vue-next'
import { ref } from 'vue'

import InvitePanel from '@/components/InvitePanel.vue'
import InvitationList from '@/components/team/InvitationList.vue'
import MemberList from '@/components/team/MemberList.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'

defineProps<{ workspaceId: string; workspaceName: string; isAdmin: boolean }>()
const inviting = ref(false)
</script>

<template>
  <Page>
    <PageHeader
      title="Team"
      description="Everyone who can work in this workspace, and who has been invited."
    >
      <button
        v-if="isAdmin"
        type="button"
        class="btn gap-1.5 btn-primary btn-sm"
        @click="inviting = true"
      >
        <UserPlus class="size-4" aria-hidden="true" />Invite
      </button>
    </PageHeader>

    <PageSection title="Members">
      <MemberList :workspace-id="workspaceId" :workspace-name="workspaceName" :is-admin="isAdmin" />
    </PageSection>

    <PageSection
      v-if="isAdmin"
      title="Pending invitations"
      description="Invitations last 7 days. Revoke one to cancel it."
    >
      <InvitationList :workspace-id="workspaceId" @invite="inviting = true" />
    </PageSection>

    <InvitePanel
      v-if="isAdmin"
      :workspace-id="workspaceId"
      :open="inviting"
      @close="inviting = false"
    />
  </Page>
</template>
