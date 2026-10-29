<script setup>
import { ref } from 'vue'
import { ArrowLeft, Save } from 'lucide-vue-next'
import OrganismAssembler from '../components/OrganismAssembler.vue'
import { modelApi } from '../api/http'
import { useI18n } from '../i18n'

const props = defineProps({ components: { type: Array, default: () => [] } })
const emit = defineEmits(['back', 'created', 'notify'])
const { t } = useI18n()
const assembly = ref(null)
const name = ref('')
const busy = ref(false)
const formError = ref('')

async function createModel() {
  formError.value = ''
  const composition = assembly.value?.buildPayload()
  if (!name.value.trim()) { formError.value = t('models.nameRequired'); return }
  const categories = new Map(props.components.map((item) => [item.id, item.category]))
  if (!composition?.components.some((item) => categories.get(item.component_id) === 'body')) { formError.value = t('models.bodyRequired'); return }
  busy.value = true
  try {
    await modelApi.create({ name: name.value.trim(), ...composition })
    emit('created')
  } catch (error) {
    emit('notify', error.message, 'error')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="builder-page">
    <header class="builder-page-header">
      <button class="icon-btn builder-back" :title="t('detail.back')" @click="emit('back')"><ArrowLeft :size="19" /></button>
      <div class="builder-title"><span class="eyebrow cyan-text">{{ t('labels.builder') }}</span><input v-model="name" :placeholder="t('models.namePlaceholder')" /></div>
      <div class="builder-save-area"><span v-if="formError" class="builder-error">{{ formError }}</span><button class="primary-btn" :disabled="busy" @click="createModel"><Save :size="15" /> {{ busy ? t('models.creating') : t('common.save') }}</button></div>
    </header>
    <OrganismAssembler ref="assembly" :components="components" />
  </section>
</template>
