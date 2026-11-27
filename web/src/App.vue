<script setup lang="ts">
import { defineAsyncComponent, onBeforeUnmount, onMounted, ref } from 'vue'
import { Database, Activity, LogOut, Menu } from 'lucide-vue-next'
import Brand from './components/Brand.vue'
import Auth from './pages/Auth/Auth.vue'
import Modal from './components/Modal.vue'
import { health, logout, status, type Session } from './api/endpoints'
import { message } from './api/client'
const Accounts = defineAsyncComponent(() => import('./pages/Accounts/Accounts.vue'))
const session = ref<Session | null>(null),
  error = ref(''),
  drawer = ref(false),
  showHealth = ref(false),
  healthText = ref('')
async function load() {
  try {
    session.value = await status()
    error.value = ''
  } catch (e) {
    error.value = message(e)
  }
}
async function signout() {
  try {
    await logout()
    drawer.value = false
    await load()
  } catch (e) {
    error.value = message(e)
  }
}
async function check() {
  showHealth.value = true
  drawer.value = false
  healthText.value = '正在检查'
  try {
    const h = await health()
    healthText.value = h.status === 'ok' ? '服务与 Redis 均已就绪' : '服务不可用'
  } catch (e) {
    healthText.value = message(e)
  }
}
onMounted(() => {
  void load()
  window.addEventListener('wbp-unauthenticated', load)
})
onBeforeUnmount(() => window.removeEventListener('wbp-unauthenticated', load))
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}<button @click="load">重试</button></p>
  <div v-if="!session && !error" class="loading">
    <Brand />
    <div class="skeleton" aria-label="正在连接" />
  </div>
  <Auth v-else-if="session && !session.authenticated" :session="session" @ready="load" />
  <div v-else-if="session" class="shell">
    <header class="top">
      <button
        class="icon"
        title="菜单"
        aria-label="打开菜单"
        :aria-expanded="drawer"
        @click="drawer = !drawer"
      >
        <Menu /></button
      ><Brand />
    </header>
    <button v-if="drawer" class="scrim" aria-label="关闭菜单" @click="drawer = false" />
    <aside :class="{ opened: drawer }">
      <Brand />
      <nav aria-label="主导航">
        <button class="selected" aria-current="page" @click="drawer = false">
          <Database />号池管理</button
        ><button @click="check"><Activity />服务状态</button>
      </nav>
      <div class="side-footer">
        <small class="mono">NOVA · v{{ session.version }}</small
        ><button class="quiet" @click="signout"><LogOut />退出登录</button>
      </div>
    </aside>
    <main class="content"><Accounts /></main>
    <Modal v-if="showHealth" title="服务状态" @close="showHealth = false"
      ><p>{{ healthText }}</p>
      <p class="muted status-version">版本 {{ session.version }}</p></Modal
    >
  </div>
</template>
<style scoped>
.loading {
  width: min(420px, 100%);
  margin: 10vh auto;
  padding: var(--space-6);
  display: grid;
  gap: var(--space-6);
}
.shell {
  min-height: 100vh;
}
aside {
  position: fixed;
  inset: 0 auto 0 0;
  width: 252px;
  display: flex;
  flex-direction: column;
  gap: var(--space-8);
  padding: var(--space-6) var(--space-4);
  background: var(--md-surface-container-low);
  z-index: 3;
  overflow: auto;
}
aside .brand {
  margin-left: var(--space-2);
}
nav {
  display: grid;
  gap: var(--space-1);
}
nav button {
  justify-content: flex-start;
  gap: var(--space-3);
  height: 48px;
  border: none;
  border-radius: var(--shape-full);
  background: transparent;
}
nav .selected {
  background: var(--md-primary-container);
  color: var(--md-on-primary-container);
}
.side-footer {
  margin-top: auto;
  border-top: 1px solid var(--md-outline-variant);
  padding-top: var(--space-4);
  display: grid;
  justify-items: start;
  gap: var(--space-2);
}
.content {
  max-width: 1480px;
  margin-left: 252px;
  padding: var(--space-6) var(--space-8);
}
.top,
.scrim {
  display: none;
}
.status-version {
  margin-top: var(--space-3);
}
@media (max-width: 1239px) and (min-width: 905px) {
  aside {
    width: 208px;
  }
  .content {
    margin-left: 208px;
    padding-inline: var(--space-6);
  }
  aside .brand {
    font-size: 16px;
  }
}
@media (max-width: 904px) {
  .content {
    margin: 0;
    padding: var(--space-6) var(--space-4);
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    background: var(--md-surface-container-low);
  }
  .top :deep(.brand) {
    font-size: 16px;
  }
  .top :deep(.brand-mark) {
    width: 32px;
    height: 32px;
  }
  aside {
    visibility: hidden;
    transform: translateX(-100%);
  }
  aside.opened {
    visibility: visible;
    transform: translateX(0);
  }
  .scrim {
    display: block;
    position: fixed;
    inset: 0;
    z-index: 2;
    background: color-mix(in srgb, var(--md-inverse-surface) 40%, transparent);
    border: 0;
    border-radius: 0;
  }
}
</style>
