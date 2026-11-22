import { createRouter, createWebHistory } from 'vue-router'

declare module 'vue-router' {
  interface RouteMeta {
    /** Reachable without a session. */
    public?: boolean
  }
}

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/sign-in', component: () => import('./pages/SignIn.vue'), meta: { public: true } },
    {
      path: '/sign-in/verify',
      component: () => import('./pages/SignInVerify.vue'),
      meta: { public: true },
    },
    { path: '/invite/:id', component: () => import('./pages/Invite.vue'), props: true },
    { path: '/', component: () => import('./pages/Workspaces.vue') },
    {
      path: '/w/:workspaceId',
      component: () => import('./pages/workspace/Layout.vue'),
      props: true,
      children: [
        { path: '', redirect: (to) => `/w/${String(to.params.workspaceId)}/projects` },
        {
          path: 'projects',
          component: () => import('./pages/workspace/Projects.vue'),
          props: true,
        },
        { path: 'members', component: () => import('./pages/workspace/Members.vue'), props: true },
        {
          path: 'invitations',
          component: () => import('./pages/workspace/Invitations.vue'),
          props: true,
        },
        { path: 'api-keys', component: () => import('./pages/workspace/ApiKeys.vue'), props: true },
        {
          path: 'settings',
          component: () => import('./pages/workspace/Settings.vue'),
          props: true,
        },
      ],
    },
    {
      path: '/:rest(.*)*',
      component: () => import('./pages/NotFound.vue'),
      meta: { public: true },
    },
  ],
})
