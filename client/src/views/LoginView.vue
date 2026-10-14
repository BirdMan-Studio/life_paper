<script setup>
import { ref } from 'vue'
import { ChevronRight, Settings, Sparkles, X } from 'lucide-vue-next'

import { getApiBase, setApiBase } from '../api/http'
import ToastMessage from '../components/ToastMessage.vue'
import { useI18n } from '../i18n'

const props = defineProps({ loading: Boolean })
const emit = defineEmits(['login', 'register', 'server-changed'])
const mode = ref('login')
const form = ref({ username: '', password: '', email: '' })
const connectionOpen = ref(false)
const serverUrl = ref(getApiBase())
const serverError = ref('')
const connectionNotice = ref('')
const { t } = useI18n()

function submit() {
  connectionNotice.value = ''
  emit(mode.value === 'login' ? 'login' : 'register', { ...form.value })
}
function openConnectionSettings() {
  serverUrl.value = getApiBase()
  serverError.value = ''
  connectionOpen.value = true
}
function saveServer() {
  const value = serverUrl.value.trim().replace(/\/$/, '')
  try {
    const url = new URL(value)
    if (!['http:', 'https:'].includes(url.protocol)) throw new Error()
  } catch {
    serverError.value = t('settings.invalidServerUrl')
    return
  }
  const changed = value !== getApiBase()
  setApiBase(value)
  serverError.value = ''
  connectionOpen.value = false
  connectionNotice.value = changed ? '' : t('settings.connectionSaved')
  if (changed) emit('server-changed')
}
</script>

<template>
  <section class="auth-screen">
    <ToastMessage :message="connectionNotice" @close="connectionNotice = ''" />
    <div class="auth-visual">
      <div class="grid-glow"></div>
      <div class="auth-copy">
        <span class="eyebrow cyan-text">LIFE PAPER / {{ t('labels.brandSubtitle') }}</span>
        <h1><template v-for="(line, index) in t('auth.evolve').split('\n')" :key="line"><br v-if="index" /><em v-if="index">{{ line }}</em><template v-else>{{ line }}</template></template></h1>
        <p>{{ t('auth.tagline') }}</p>
      </div>
    </div>
    <div class="auth-panel">
      <div class="auth-logo"><div class="brand-mark"><Sparkles :size="17" /></div><strong>LIFE PAPER</strong></div>
      <div class="auth-form">
        <span class="eyebrow">{{ mode === 'login' ? t('auth.loginEyebrow') : t('auth.registerEyebrow') }}</span>
        <h2>{{ mode === 'login' ? t('auth.welcome') : t('auth.createAccount') }}</h2>
        <p>{{ mode === 'login' ? t('auth.loginHint') : t('auth.registerHint') }}</p>
        <form @submit.prevent="submit">
          <label>{{ t('auth.username') }}<input v-model="form.username" :placeholder="t('auth.usernamePlaceholder')" required /></label>
          <label v-if="mode === 'register'">{{ t('auth.email') }}<input v-model="form.email" type="email" placeholder="name@example.com" required /></label>
          <label>{{ t('auth.password') }}<input v-model="form.password" type="password" :placeholder="t('auth.passwordPlaceholder')" required /></label>
          <button class="primary-btn submit" :disabled="props.loading">{{ props.loading ? t('auth.connecting') : mode === 'login' ? t('auth.enter') : t('auth.creating') }}<ChevronRight :size="17" /></button>
        </form>
        <button class="switch-auth" @click="mode = mode === 'login' ? 'register' : 'login'">{{ mode === 'login' ? t('auth.noAccount') : t('auth.hasAccount') }}</button>
      </div>
      <button class="icon-btn login-settings-btn" :title="t('settings.connectionSettings')" :aria-label="t('settings.connectionSettings')" @click="openConnectionSettings"><Settings :size="18" /></button>
    </div>

    <div v-if="connectionOpen" class="modal-backdrop" @click.self="connectionOpen = false">
      <div class="create-modal connection-modal" role="dialog" aria-modal="true" :aria-label="t('settings.connectionSettings')">
        <div class="modal-title"><div><span class="eyebrow cyan-text">{{ t('settings.connectionEyebrow') }}</span><h3>{{ t('settings.connectionSettings') }}</h3></div><button class="icon-btn" :title="t('common.cancel')" @click="connectionOpen = false"><X :size="18" /></button></div>
        <label class="modal-field">{{ t('settings.serverUrl') }}<input v-model="serverUrl" type="url" :placeholder="t('settings.serverUrlPlaceholder')" autofocus @keyup.enter="saveServer" /></label>
        <div v-if="serverError" class="auth-notice">{{ serverError }}</div>
        <div class="modal-actions"><button class="ghost-btn" @click="connectionOpen = false">{{ t('common.cancel') }}</button><button class="primary-btn" @click="saveServer">{{ t('settings.saveConnection') }}</button></div>
      </div>
    </div>
  </section>
</template>
