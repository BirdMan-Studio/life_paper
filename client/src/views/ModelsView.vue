<script setup>
import { BrainCircuit, ChevronRight, Plus } from 'lucide-vue-next'
import { useI18n } from '../i18n'

const props = defineProps({ models: Array, components: Array, loading: Boolean })
const emit = defineEmits(['open', 'create'])
const { t, locale } = useI18n()

function componentCount(model) { const value = model.composition?.components; return Array.isArray(value) ? value.length : Object.keys(value || {}).length }
</script>

<template>
  <section class="content">
    <div class="page-intro">
      <div><span class="eyebrow cyan-text">{{ t('labels.modelsEyebrow') }}</span><h2>{{ t('nav.models') }}</h2><p>{{ t('models.subtitle') }}</p></div>
      <button class="primary-btn" @click="emit('create')"><Plus :size="16" /> {{ t('models.create') }}</button>
    </div>
    <div v-if="loading" class="empty-state">{{ t('models.loading') }}</div>
    <div v-else class="table-card">
      <div class="table-head"><span>{{ t('models.name') }}</span><span>{{ t('models.components') }}</span><span>{{ t('models.created') }}</span><span>{{ t('models.updated') }}</span><span></span></div>
      <div v-if="!models.length" class="panel-empty">{{ t('models.empty') }}</div>
      <div v-for="model in models" :key="model.id" class="table-row" @dblclick="emit('open', model)">
        <div class="model-name"><span class="model-icon"><BrainCircuit :size="17" /></span><b>{{ model.name }}</b></div>
        <span>{{ t('common.count', { count: componentCount(model) }) }}</span>
        <span>{{ new Date(model.created_at).toLocaleString(locale === 'zh_CN' ? 'zh-CN' : 'en-US') }}</span>
        <span>{{ new Date(model.updated_at).toLocaleString(locale === 'zh_CN' ? 'zh-CN' : 'en-US') }}</span>
        <button class="icon-btn" :title="t('models.open')" @click="emit('open', model)"><ChevronRight :size="16" /></button>
      </div>
    </div>
  </section>
</template>
