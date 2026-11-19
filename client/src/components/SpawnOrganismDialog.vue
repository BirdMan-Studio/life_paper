<script setup>
import { computed, onMounted, ref } from 'vue'
import { Plus, RefreshCw, Trash2, X } from 'lucide-vue-next'
import { pythonApi } from '../api/http'

const props = defineProps({ model: { type: Object, required: true }, busy: Boolean })
const emit = defineEmits(['close', 'confirm'])
const catalog = ref(null)
const loading = ref(true)
const error = ref('')
const rows = ref([])
const dialog = ref(null)
let nextId = 0
const containers = computed(() => Object.entries(props.model.composition?.components || {}).flatMap(([id, part]) => {
  const definition = catalog.value?.components?.find(item => item.id === part.component)
  const properties = definition?.properties || {}
  return properties.non_gas_capacity_mg > 0 || properties.gas_capacity_ul > 0
    ? [{ id, label: `${definition.name || part.component} · ${id.slice(0, 8)}`, ...properties }] : []
}))
const substances = computed(() => catalog.value?.substances || [])
const usage = computed(() => Object.fromEntries(containers.value.map(container => [container.id, {
  non_gas_contents_mg: rows.value.filter(row => row.component === container.id && row.kind === 'non_gas_contents_mg').reduce((sum, row) => sum + Number(row.amount), 0),
  gas_contents_ul: rows.value.filter(row => row.component === container.id && row.kind === 'gas_contents_ul').reduce((sum, row) => sum + Number(row.amount), 0),
}])))
const validation = computed(() => {
  for (const row of rows.value) {
    if (!containers.value.some(item => item.id === row.component) || !substances.value.some(item => item.id === row.element)) return '请选择容器和物质'
    const amount = Number(row.amount)
    if (!Number.isFinite(amount) || amount <= 0 || (row.kind === 'gas_contents_ul' && !Number.isSafeInteger(amount))) return '数量须大于 0，气体须填写整数微升'
  }
  for (const container of containers.value) {
    const used = usage.value[container.id]
    if (used.non_gas_contents_mg > (container.non_gas_capacity_mg || 0) || used.gas_contents_ul > (container.gas_capacity_ul || 0)) return `${container.label} 超出容量`
  }
  return ''
})
async function loadCatalog() {
  loading.value = true
  error.value = ''
  try { catalog.value = (await pythonApi.documentation()).data }
  catch (cause) { error.value = cause.message }
  finally { loading.value = false }
}
function addRow() {
  const container = containers.value[0]
  rows.value.push({ id: nextId++, component: container.id, kind: container.non_gas_capacity_mg > 0 ? 'non_gas_contents_mg' : 'gas_contents_ul', element: substances.value.find(item => item.id === 'monosaccharide')?.id || substances.value[0]?.id || '', amount: '' })
}
function confirm() {
  if (props.busy || loading.value || error.value || validation.value) return
  const contents = {}
  for (const row of rows.value) {
    const container = contents[row.component] ||= { non_gas_contents_mg: {}, gas_contents_ul: {} }
    container[row.kind][row.element] = (container[row.kind][row.element] || 0) + Number(row.amount)
  }
  emit('confirm', contents)
}
function close() { if (!props.busy) emit('close') }
function trapFocus(event) {
  if (event.key !== 'Tab') return
  const controls = [...dialog.value.querySelectorAll('button:not(:disabled), input:not(:disabled), select:not(:disabled)')]
  const first = controls[0], last = controls.at(-1)
  if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.value)) { event.preventDefault(); last?.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
}
onMounted(() => { dialog.value.focus(); loadCatalog() })
</script>

<template>
  <div class="modal-backdrop" @click.self="close">
    <form ref="dialog" class="spawn-dialog" role="dialog" aria-modal="true" aria-labelledby="spawn-title" tabindex="-1" @submit.prevent="confirm" @keydown.esc="close" @keydown="trapFocus">
      <div class="modal-title"><h3 id="spawn-title">添加生物 · {{ model.name }}</h3><button type="button" class="icon-btn" title="关闭" aria-label="关闭" :disabled="busy" @click="close"><X :size="18" /></button></div>
      <div v-if="loading" role="status">正在加载容器和物质…</div>
      <div v-else-if="error" role="alert">{{ error }} <button type="button" class="icon-btn" title="重试" @click="loadCatalog"><RefreshCw :size="16" /></button></div>
      <template v-else>
        <h4>初始物资</h4>
        <div class="spawn-capacities">
          <div v-for="container in containers" :key="container.id"><strong>{{ container.label }}</strong><span>固液 {{ usage[container.id].non_gas_contents_mg || 0 }} / {{ container.non_gas_capacity_mg || 0 }} mg</span><span>气体 {{ usage[container.id].gas_contents_ul || 0 }} / {{ container.gas_capacity_ul || 0 }} µL</span></div>
          <span v-if="!containers.length">无可用容器</span>
        </div>
        <fieldset :disabled="busy" class="spawn-fields">
          <div v-for="row in rows" :key="row.id" class="spawn-row">
            <label>组件容器<select v-model="row.component" required><option v-for="container in containers" :key="container.id" :value="container.id">{{ container.label }}</option></select></label>
            <label>储存类型<select v-model="row.kind"><option value="non_gas_contents_mg">固体 / 液体 (mg)</option><option value="gas_contents_ul">气体 (µL)</option></select></label>
            <label>物质<select v-model="row.element" aria-label="物质" required><option v-for="element in substances" :key="element.id" :value="element.id">{{ element.name }} ({{ element.id }})</option></select></label>
            <label>数量<input v-model="row.amount" type="number" min="0" :step="row.kind === 'gas_contents_ul' ? '1' : 'any'" required /></label>
            <button type="button" class="icon-btn" title="移除物资" aria-label="移除物资" @click="rows = rows.filter(item => item.id !== row.id)"><Trash2 :size="16" /></button>
          </div>
          <button type="button" class="ghost-btn" :disabled="!containers.length || !substances.length" @click="addRow"><Plus :size="15" />添加物资</button>
        </fieldset>
        <p v-if="validation" class="spawn-error" role="alert">{{ validation }}</p>
      </template>
      <div class="modal-actions"><button type="button" class="ghost-btn" :disabled="busy" @click="close">取消</button><button type="submit" class="primary-btn" :disabled="busy || loading || !!error || !!validation"><Plus :size="15" />{{ busy ? '创建中…' : '创建实例' }}</button></div>
    </form>
  </div>
</template>

<style scoped>
.spawn-dialog{width:min(760px,calc(100vw - 32px));max-height:85vh;overflow:auto;padding:24px;background:#111c2a;border:1px solid var(--line);color:#d9e5ef}.spawn-dialog h3{font-size:18px;overflow-wrap:anywhere}.spawn-dialog h4{font-size:13px}.spawn-capacities{display:grid;gap:8px;margin:14px 0 20px;font-size:11px;color:#9fb0bf}.spawn-capacities>div{display:flex;flex-wrap:wrap;gap:6px 16px;padding-bottom:8px;border-bottom:1px solid var(--line)}.spawn-capacities strong{flex-basis:100%;overflow-wrap:anywhere}.spawn-fields{border:0;padding:0;min-width:0}.spawn-row{display:grid;grid-template-columns:minmax(0,1.4fr) minmax(0,1fr) minmax(0,1.3fr) minmax(0,.7fr) 32px;gap:8px;align-items:end;margin-bottom:14px}.spawn-row label{display:flex;flex-direction:column;gap:6px;font-size:11px;color:#9fb0bf;min-width:0}.spawn-row input,.spawn-row select{width:100%;min-width:0;height:36px;border:1px solid var(--line);background:#0c1520;color:#d9e5ef;padding:0 6px}.spawn-row .icon-btn{height:36px;width:32px}.spawn-error{margin-top:12px;color:#f88b8b;font-size:12px;overflow-wrap:anywhere}.spawn-fields>.ghost-btn,.modal-actions button{display:inline-flex;gap:6px;align-items:center}@media(max-width:600px){.spawn-dialog{padding:16px}.spawn-row{grid-template-columns:minmax(0,1fr) 32px;border-bottom:1px solid var(--line);padding-bottom:12px}.spawn-row label{grid-column:1}.spawn-row>.icon-btn{grid-column:2;grid-row:1}}
</style>
