<script setup lang="ts">
import { UserPlus } from 'lucide-vue-next'
import { ref } from 'vue'

import InvitePanel from '@/components/InvitePanel.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import InvitationList from '@/components/team/InvitationList.vue'
import MemberList from '@/components/team/MemberList.vue'

defineProps<{ workspaceId: string; workspaceName: string; isAdmin: boolean }>()
const inviting = ref(false)
</script>

<template>
  <div>
    <PageHeader
      :section="workspaceName"
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

    <section aria-labelledby="members-heading">
      <h2 id="members-heading" class="mb-3 text-sm font-semibold">Members</h2>
      <MemberList :workspace-id="workspaceId" :workspace-name="workspaceName" :is-admin="isAdmin" />
    </section>

    <section v-if="isAdmin" class="mt-8" aria-labelledby="invitations-heading">
      <h2 id="invitations-heading" class="mb-3 text-sm font-semibold">Pending invitations</h2>
      <p class="mb-3 text-sm text-base-content/80">
        Invitations last 7 days. Revoke one to cancel it.
      </p>
      <InvitationList :workspace-id="workspaceId" @invite="inviting = true" />
    </section>

    <InvitePanel
      v-if="isAdmin"
      :workspace-id="workspaceId"
      :open="inviting"
      @close="inviting = false"
    />
  </div>
</template>
