<script setup>
import { computed, ref } from 'vue'
import { BrainCircuit, ChevronRight, Plus, X } from 'lucide-vue-next'
import { modelApi } from '../api/http'
import { useI18n } from '../i18n'

const props = defineProps({ models: Array, components: Array, loading: Boolean })
const emit = defineEmits(['open', 'created', 'notify'])
const creating = ref(false)
const busy = ref(false)
const formError = ref('')
const name = ref('')
const bodyId = ref('')
const bodyInstanceId = ref('')
const selected = ref([])
const { t, locale } = useI18n()
const bodies = computed(() => props.components.filter((item) => item.category === 'body'))
const addable = computed(() => props.components.filter((item) => item.category !== 'body'))

function slotsOf(component) { return component?.slots?.counts || component?.slots || {} }
function componentCount(model) { const value = model.composition?.components; return Array.isArray(value) ? value.length : Object.keys(value || {}).length }
function openCreate() {
  creating.value = true
  formError.value = ''
  name.value = ''
  selected.value = []
  bodyId.value = bodies.value[0]?.id || ''
  bodyInstanceId.value = crypto.randomUUID()
}
function addComponent(componentId) {
  selected.value.push({ id: crypto.randomUUID(), componentId, parentId: bodyInstanceId.value })
}
function removeComponent(instanceId) {
  selected.value = selected.value
    .filter((item) => item.id !== instanceId)
    .map((item) => item.parentId === instanceId ? { ...item, parentId: bodyInstanceId.value } : item)
}
function selectedFor(componentId) { return selected.value.filter((item) => item.componentId === componentId) }
function parentOptions(index) {
  const body = bodies.value.find((item) => item.id === bodyId.value)
  return [
    { id: bodyInstanceId.value, component: body },
    ...selected.value.slice(0, index).map((item) => ({
      id: item.id,
      component: props.components.find((component) => component.id === item.componentId),
    })),
  ].filter((item) => item.component)
}
async function createModel() {
  formError.value = ''
  if (!name.value.trim() || !bodyId.value) { formError.value = t('models.required'); return }
  const body = props.components.find((item) => item.id === bodyId.value)
  const parts = [
    { id: bodyInstanceId.value, component: body },
    ...selected.value.map((item) => ({
      ...item,
      component: props.components.find((component) => component.id === item.componentId),
    })).filter((item) => item.component),
  ]
  const instances = parts.map((item) => ({ id: item.id, component_id: item.component.id }))
  const partById = new Map(parts.map((item) => [item.id, item]))
  const connections = []
  const usedSlots = new Map()
  for (let index = 1; index < parts.length; index += 1) {
    const child = parts[index]
    const parent = partById.get(child.parentId) || parts[0]
    const parentSlotKey = `${parent.id}:${child.component.category}`
    const childSlotKey = `${child.id}:${parent.component.category}`
    const parentUsed = usedSlots.get(parentSlotKey) || 0
    const childUsed = usedSlots.get(childSlotKey) || 0
    const parentCapacity = slotsOf(parent.component)[child.component.category] || 0
    const childCapacity = slotsOf(child.component)[parent.component.category] || 0
    if (parentCapacity <= parentUsed || childCapacity <= childUsed) {
      formError.value = t('models.slots', { parent: parent.component.name, child: child.component.name })
      return
    }
    usedSlots.set(parentSlotKey, parentUsed + 1)
    usedSlots.set(childSlotKey, childUsed + 1)
    connections.push({ first: parent.id, second: child.id })
  }
  busy.value = true
  try {
    await modelApi.create({ name: name.value.trim(), components: instances, connections })
    creating.value = false
    emit('created')
  } catch (error) {
    emit('notify', error.message, 'error')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="content">
    <div class="page-intro">
      <div><span class="eyebrow cyan-text">{{ t('labels.modelsEyebrow') }}</span><h2>{{ t('nav.models') }}</h2><p>{{ t('models.subtitle') }}</p></div>
      <button class="primary-btn" @click="openCreate"><Plus :size="16" /> {{ t('models.create') }}</button>
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
    <div v-if="creating" class="modal-backdrop" @click.self="creating = false">
      <div class="create-modal">
        <div class="modal-title"><div><span class="eyebrow cyan-text">{{ t('labels.builder') }}</span><h3>{{ t('models.builder') }}</h3></div><button class="icon-btn" @click="creating = false"><X :size="18" /></button></div>
        <label class="modal-field">{{ t('models.name') }}<input v-model="name" :placeholder="t('models.namePlaceholder')" /></label>
        <label class="modal-field">{{ t('models.body') }}<select v-model="bodyId"><option disabled value="">{{ t('models.selectBody') }}</option><option v-for="component in bodies" :key="component.id" :value="component.id">{{ component.name }}</option></select></label>
        <div class="component-picker">
          <span class="eyebrow">{{ t('models.available') }}</span>
          <div v-for="component in addable" :key="component.id" class="builder-component">
            <button class="component-option" @click="addComponent(component.id)"><span>{{ component.name }}</span><small>{{ component.category }} · {{ t('models.addComponent') }}</small></button>
            <div v-for="instance in selectedFor(component.id)" :key="instance.id" class="builder-component-instance">
              <select v-model="instance.parentId" class="parent-select">
                <option v-for="parent in parentOptions(selected.findIndex((item) => item.id === instance.id))" :key="parent.id" :value="parent.id">{{ t('models.connectTo', { name: parent.component.name }) }}</option>
              </select>
              <button class="icon-btn" :title="t('models.removeComponent')" @click="removeComponent(instance.id)"><X :size="14" /></button>
            </div>
          </div>
          <div v-if="!addable.length" class="panel-empty">{{ t('models.noneAvailable') }}</div>
        </div>
        <div v-if="formError" class="auth-notice">{{ formError }}</div>
        <div class="modal-actions"><button class="ghost-btn" @click="creating = false">{{ t('common.cancel') }}</button><button class="primary-btn" :disabled="busy" @click="createModel">{{ busy ? t('models.creating') : t('models.create') }}</button></div>
      </div>
    </div>
  </section>
</template>
