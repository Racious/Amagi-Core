import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/dashboard' },
    { path: '/dashboard', name: 'dashboard', component: () => import('../pages/DashboardPage.vue') },
    { path: '/projects', name: 'projects', component: () => import('../pages/ProjectsPage.vue') },
    { path: '/learn', name: 'learn', component: () => import('../pages/LearnPage.vue') },
    { path: '/review', name: 'review', component: () => import('../pages/ReviewQueuePage.vue') },
    { path: '/skills', name: 'skills', component: () => import('../pages/SkillsPage.vue') },
    { path: '/sync', name: 'sync', component: () => import('../pages/SyncPreviewPage.vue') },
    { path: '/workflows', name: 'workflows', component: () => import('../pages/WorkflowsPage.vue') },
    { path: '/run', name: 'run', component: () => import('../pages/RunBridgePage.vue') },
    { path: '/settings', name: 'settings', component: () => import('../pages/SettingsPage.vue') },
  ],
})

export default router
