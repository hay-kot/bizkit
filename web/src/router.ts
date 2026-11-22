import { createRouter, createWebHistory } from 'vue-router'

import AuthLayout from './layouts/AuthLayout.vue'
import HomeLayout from './layouts/HomeLayout.vue'
import WorkspaceLayout from './layouts/WorkspaceLayout.vue'
import Home from './pages/Home.vue'
import Invite from './pages/Invite.vue'
import NotFound from './pages/NotFound.vue'
import SignIn from './pages/SignIn.vue'
import SignInVerify from './pages/SignInVerify.vue'
import ApiKeys from './pages/workspace/ApiKeys.vue'
import Invitations from './pages/workspace/Invitations.vue'
import Members from './pages/workspace/Members.vue'
import Projects from './pages/workspace/Projects.vue'
import Settings from './pages/workspace/Settings.vue'

declare module 'vue-router' {
  interface RouteMeta {
    /** Reachable without a session. */
    public?: boolean
  }
}

// Pages load eagerly: the app is small, and lazy chunks made every first
// visit to a page flash blank.
export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/sign-in',
      component: AuthLayout,
      meta: { public: true },
      children: [
        { path: '', component: SignIn, meta: { public: true } },
        { path: 'verify', component: SignInVerify, meta: { public: true } },
      ],
    },
    {
      path: '/',
      component: HomeLayout,
      children: [
        { path: '', component: Home },
        { path: 'invite/:id', component: Invite, props: true },
      ],
    },
    {
      path: '/w/:workspaceId',
      component: WorkspaceLayout,
      props: true,
      children: [
        { path: '', redirect: (to) => `/w/${String(to.params.workspaceId)}/projects` },
        { path: 'projects', component: Projects },
        { path: 'members', component: Members },
        { path: 'invitations', component: Invitations },
        { path: 'api-keys', component: ApiKeys },
        { path: 'settings', component: Settings },
      ],
    },
    { path: '/:rest(.*)*', component: NotFound, meta: { public: true } },
  ],
})
