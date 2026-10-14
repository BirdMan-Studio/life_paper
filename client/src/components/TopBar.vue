<script setup>
import { Activity, LogOut } from 'lucide-vue-next'
import { useI18n } from '../i18n'
defineProps({ label: String, profile: Object, wallet: Object })
defineEmits(['logout'])
const { t } = useI18n()
</script>
<template><header class="topbar"><div class="top-actions"><template v-if="wallet"><div class="resource"><span class="resource-icon energy"><Activity :size="15" /></span><strong>{{ wallet.energy_coins.toLocaleString() }}</strong><small>{{ t('top.energy') }}</small></div><div class="resource"><span class="resource-icon gold">✦</span><strong>{{ wallet.gold_coins }}</strong><small>{{ t('top.gold') }}</small></div></template><div v-if="profile" class="profile"><div class="avatar">{{ profile.username?.slice(0, 1).toUpperCase() }}</div><span>{{ profile.username }}</span><button class="icon-btn" @click="$emit('logout')" :title="t('top.logout')"><LogOut :size="16" /></button></div></div></header></template>
