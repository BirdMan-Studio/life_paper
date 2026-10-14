<script setup>
import { ChevronRight, FlaskConical, Globe2, Menu, Settings, Sparkles, BrainCircuit } from 'lucide-vue-next'
import { useI18n } from '../i18n'
defineProps({ route: String, compact: Boolean })
const emit = defineEmits(['navigate', 'toggle'])
const { t } = useI18n()
const items = [
  { id: 'world', key: 'nav.world', icon: Globe2 },
  { id: 'creatures', key: 'nav.creatures', icon: FlaskConical },
  { id: 'models', key: 'nav.models', icon: BrainCircuit },
  { id: 'settings', key: 'nav.settings', icon: Settings },
]
</script>
<template>
  <aside class="sidebar"><div class="brand"><div class="brand-mark"><Sparkles :size="18" /></div><div v-if="!compact"><strong>LIFE PAPER</strong><span>{{ t('labels.brandSubtitle') }}</span></div><button class="icon-btn menu-toggle" @click="emit('toggle')"><Menu :size="17" /></button></div><div class="nav-label" v-if="!compact">{{ t('nav.operations') }}</div><nav><button v-for="item in items" :key="item.id" class="nav-item" :class="{ active: route === item.id }" @click="emit('navigate', item.id)"><component :is="item.icon" :size="19" /><span v-if="!compact"><b>{{ t(item.key) }}</b></span><ChevronRight v-if="route === item.id && !compact" class="nav-arrow" :size="15" /></button></nav><div class="sidebar-footer"><div class="status-dot"></div><span v-if="!compact">{{ t('nav.connected') }}</span></div></aside>
</template>
