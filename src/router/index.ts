import { createRouter, createWebHistory } from "vue-router";
import ChatView from "../views/ChatView.vue";
import DesktopView from "../views/DesktopView.vue";

const routes = [
  { path: "/", component: ChatView },
  { path: "/chat", component: ChatView },
  { path: "/desktop", component: DesktopView },
  { path: "/admin", component: () => import("../views/AdminView.vue") },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

export default router;
