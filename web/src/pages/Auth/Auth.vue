<script setup lang="ts">
import { ref } from 'vue'
import { KeyRound, ArrowRight } from 'lucide-vue-next'
import Brand from '../../components/Brand.vue'
import { login, setupAdmin, type Session } from '../../api/endpoints'
import { HttpError, message } from '../../api/client'
const props = defineProps<{ session: Session }>()
const emit = defineEmits<{ ready: [] }>()
const password = ref(''),
  confirmation = ref(''),
  error = ref(''),
  busy = ref(false)
async function submit() {
  if (busy.value) return
  error.value = ''
  const setup = props.session.needs_setup
  if (setup && password.value !== confirmation.value) {
    error.value = '两次输入的管理员密钥不一致'
    return
  }
  busy.value = true
  try {
    if (setup) await setupAdmin(password.value, confirmation.value)
    else await login(password.value)
    password.value = ''
    confirmation.value = ''
    emit('ready')
  } catch (e) {
    if (setup && e instanceof HttpError && e.status === 409) {
      password.value = ''
      confirmation.value = ''
      error.value = '管理员密钥已设置，请直接登录'
      emit('ready')
    } else {
      error.value = message(e)
    }
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <main class="auth">
    <Brand />
    <section>
      <div class="eyebrow"><KeyRound :size="16" />管理控制台</div>
      <h1>{{ session.needs_setup ? '设置管理员密钥' : '管理员登录' }}</h1>
      <form @submit.prevent="submit" @input="error = ''">
        <label>
          管理员密钥
          <input
            v-model="password"
            type="password"
            name="password"
            required
            :minlength="session.needs_setup ? 10 : 1"
            :autocomplete="session.needs_setup ? 'new-password' : 'current-password'"
            :aria-describedby="session.needs_setup ? 'key-length' : undefined"
          />
        </label>
        <label v-if="session.needs_setup">
          确认管理员密钥
          <input
            v-model="confirmation"
            type="password"
            name="password_confirm"
            required
            autocomplete="new-password"
          />
        </label>
        <small v-if="session.needs_setup" id="key-length">至少 10 个字符，最多 72 字节。</small>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
        <button class="primary" :disabled="busy">
          {{ busy ? '正在验证' : session.needs_setup ? '设置并登录' : '登录' }}<ArrowRight />
        </button>
      </form>
      <small v-if="session.needs_setup" class="storage-note">
        本项目不加密本地数据。<code>access_account/</code> 与 Redis 数据等同于凭证本身，请自行限制目录权限、做好备份、不要把数据目录暴露给他人。
      </small>
    </section>
    <small class="mono">NOVA · v{{ session.version }}</small>
  </main>
</template>
<style scoped>
.auth {
  max-width: 440px;
  margin: 0 auto;
  padding: clamp(48px, 13vh, 140px) var(--space-6) var(--space-8);
}
section {
  margin: var(--space-10) 0 var(--space-8);
}
h1 {
  margin: var(--space-3) 0 var(--space-6);
}
.eyebrow {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--md-on-surface-variant);
}
section button {
  height: 44px;
}
.storage-note {
  display: block;
  margin-top: var(--space-4);
}
</style>
