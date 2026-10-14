<script setup>
import { Globe2, UserRound } from 'lucide-vue-next'
import { reactive } from 'vue'
import ActionDialog from '../components/ActionDialog.vue'

import { useI18n } from '../i18n'

defineProps({ profile: Object })
const emit = defineEmits(['saved', 'delete-account'])
const { locale, t, setLocale } = useI18n()
const dialog = reactive({ open: false })

function saveLanguage(event) { setLocale(event.target.value); emit('saved', t('settings.languageSaved')) }
function requestDelete() { dialog.open = true }
function cancelDelete() { dialog.open = false }
function confirmDelete() { dialog.open = false; emit('delete-account') }
</script>

<template>
  <section class="content">
    <div class="page-intro"><div><span class="eyebrow cyan-text">{{ t('labels.systemSettings') }}</span><h2>{{ t('nav.settings') }}</h2><p>{{ t('settings.subtitle') }}</p></div></div>
    <div class="settings-grid">
      <div class="setting-panel"><h3>{{ t('settings.preferences') }}</h3><label>{{ t('settings.language') }}<select :value="locale" @change="saveLanguage"><option value="zh_CN">简体中文</option><option value="en_US">English</option></select></label></div>
      <div class="setting-panel"><h3>{{ t('settings.account') }}</h3><div class="account-line"><UserRound :size="17" /><span>{{ t('settings.username') }}</span><b>{{ profile?.username }}</b></div><div class="account-line"><Globe2 :size="17" /><span>{{ t('settings.email') }}</span><b>{{ profile?.email }}</b></div><div class="account-line"><Globe2 :size="17" /><span>{{ t('settings.client') }}</span><b>Tauri Desktop · v0.1</b></div><button class="danger-btn account-delete" @click="requestDelete">{{ t('settings.deleteAccount') }}</button></div>
    </div>
    <ActionDialog v-if="dialog.open" :title="t('settings.deleteAccount')" :message="t('settings.deleteConfirm')" :confirm-text="t('common.delete')" :cancel-text="t('common.cancel')" destructive @confirm="confirmDelete" @cancel="cancelDelete" />
  </section>
</template>
