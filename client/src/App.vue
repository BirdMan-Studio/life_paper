<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import AppShell from './components/AppShell.vue'
import LoginView from './views/LoginView.vue'
import WorldView from './views/WorldView.vue'
import CreaturesView from './views/CreaturesView.vue'
import ModelsView from './views/ModelsView.vue'
import SettingsView from './views/SettingsView.vue'
import ModelDetailView from './views/ModelDetailView.vue'
import ModelBuilderView from './views/ModelBuilderView.vue'
import ToastMessage from './components/ToastMessage.vue'
import { useSession } from './stores/session'
import { useDataStore } from './stores/data'
import { useI18n } from './i18n'

const session = useSession()
const data = useDataStore()
const { t } = useI18n()
const route = ref('world')
const compact = ref(false)
const notice = ref('')
const noticeTone = ref('info')
const selectedModel = ref(null)
const currentLabel = computed(() => route.value === 'model-detail' ? selectedModel.value?.name || t('nav.models') : route.value === 'model-builder' ? t('models.builder') : t(`nav.${route.value}`))
const isReady = computed(() => session.authenticated.value)
const toastMessage = computed(() => session.error.value || notice.value || data.error.value)
const toastTone = computed(() => session.error.value || data.error.value ? 'error' : noticeTone.value)

async function enterApp() { await data.load() }
async function login(payload) { if (await session.login(payload)) await enterApp() }
async function register(payload) { if (await session.register(payload)) saveNotice(t('notice.registered')) }
async function logout() { await session.logout(); data.reset(); route.value = 'world' }
async function deleteAccount() { try { await session.deleteAccount(); data.reset(); saveNotice(t('notice.deletedAccount')); route.value = 'world' } catch (error) { saveNotice(error.message, 'error') } }
async function toggleWorld() { try { await data.setActiveWorldStatus() } catch (error) { saveNotice(error.message, 'error') } }
async function tutorial() { try { await data.ensureTutorial() } catch (error) { saveNotice(error.message, 'error') } }
function saveNotice(message, tone = 'info') { notice.value = message; noticeTone.value = tone }
function closeToast() { if (session.error.value) session.clearError(); else if (notice.value) notice.value = ''; else data.clearError() }
function serverChanged() { session.clearLocalSession(); data.reset(); selectedModel.value = null; route.value = 'world'; saveNotice(t('notice.serverChanged')) }
function openModel(model) { selectedModel.value = model; route.value = 'model-detail' }
async function modelDeleted() { await data.load(); selectedModel.value = null; route.value = 'models'; saveNotice(t('notice.modelDeleted')) }
async function modelCreated() { await data.load(); route.value = 'models'; saveNotice(t('models.createdNotice')) }
async function unlockComponent(id) { try { await data.unlockComponent(id); saveNotice(t('notice.componentUnlocked')) } catch (error) { saveNotice(error.message, 'error') } }
function expireSession() { session.clearLocalSession(); data.reset(); selectedModel.value = null; route.value = 'world'; saveNotice(t('notice.sessionExpired'), 'error') }
onMounted(async () => {
  window.addEventListener('life-paper:unauthorized', expireSession)
  if (await session.restore()) await enterApp()
})
onBeforeUnmount(() => window.removeEventListener('life-paper:unauthorized', expireSession))
</script>

<template>
  <ToastMessage :message="toastMessage" :tone="toastTone" @close="closeToast" />
  <LoginView v-if="!isReady" :loading="session.loading.value" @login="login" @register="register" @server-changed="serverChanged" />
  <AppShell v-else :route="route" :compact="compact" :profile="session.profile.value" :wallet="data.wallet.value" :label="currentLabel" @navigate="route = $event" @toggle="compact = !compact" @logout="logout">
    <WorldView v-if="route === 'world'" :worlds="data.worlds.value" :active-world="data.activeWorld.value" :ground="data.ground.value" :wallet="data.wallet.value" :models="data.models.value" :loading="data.loading.value" @select-world="data.selectWorld" @toggle-world="toggleWorld" @tutorial="tutorial" @notify="saveNotice" />
    <CreaturesView v-else-if="route === 'creatures'" :components="data.unlockedComponents.value" :locked="data.lockedComponents.value" :models="data.models.value" :loading="data.loading.value" :unlock-action="unlockComponent" />
    <ModelsView v-else-if="route === 'models'" :models="data.models.value" :components="data.unlockedComponents.value" :loading="data.loading.value" @open="openModel" @create="route = 'model-builder'" />
    <ModelBuilderView v-else-if="route === 'model-builder'" :components="data.unlockedComponents.value" @back="route = 'models'" @created="modelCreated" @notify="saveNotice" />
    <ModelDetailView v-else-if="route === 'model-detail' && selectedModel" :model="selectedModel" @back="route = 'models'" @deleted="modelDeleted" />
    <SettingsView v-else :profile="session.profile.value" @saved="saveNotice" @delete-account="deleteAccount" />
  </AppShell>
</template>
