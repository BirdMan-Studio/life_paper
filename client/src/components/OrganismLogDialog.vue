<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { RefreshCw, X } from 'lucide-vue-next'
import { organismApi } from '../api/http'
import { useI18n } from '../i18n'

const props = defineProps({ worldId: { type: String, required: true }, organism: { type: Object, required: true } })
const emit = defineEmits(['close', 'updated'])
const { t } = useI18n()
const dialog = ref(null)
const logBody = ref(null)
const loading = ref(false)
const error = ref('')
const state = ref(null)
const reports = computed(() => state.value?.history?.length ? state.value.history : (state.value?.last_run ? [state.value.last_run] : []))
const output = computed(() => reports.value.flatMap(report => [
  ...(report.logs ? [{ text: report.logs, failed: false }] : []),
  ...(report.error ? [{ text: report.error, failed: true }] : []),
]).map(part => ({ ...part, text: part.text.endsWith('\n') ? part.text : `${part.text}\n` })))
let poll
let request = 0

async function refresh() {
  const current = ++request
  loading.value = true
  error.value = ''
  try {
    const response = await organismApi.program(props.worldId, props.organism.id).status()
    if (current !== request) return
    state.value = response.data.state
    emit('updated', state.value)
  } catch (cause) {
    if (current === request) error.value = cause.message
  } finally {
    if (current === request) {
      loading.value = false
      await nextTick()
      if (current === request && logBody.value) logBody.value.scrollTop = logBody.value.scrollHeight
    }
  }
}

function closeOnBackdrop(event) {
  if (event.target !== dialog.value) return
  const bounds = dialog.value.getBoundingClientRect()
  if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) emit('close')
}
watch([() => props.worldId, () => props.organism.id], () => { state.value = null; refresh() }, { immediate: true })
onMounted(() => { dialog.value.showModal(); poll = window.setInterval(() => { if (!loading.value) refresh() }, 1500) })
onBeforeUnmount(() => { request += 1; if (poll) window.clearInterval(poll); dialog.value?.close() })
</script>

<template>
  <dialog ref="dialog" class="organism-log-dialog" :aria-label="t('organismActions.logs')" @cancel.prevent="emit('close')" @click="closeOnBackdrop">
    <header class="organism-log-header">
      <div><h3>{{ organism.name }}</h3></div>
      <button class="icon-btn" :title="t('organismActions.refreshLogs')" :aria-label="t('organismActions.refreshLogs')" :disabled="loading" @click="refresh"><RefreshCw :size="16" /></button>
      <button class="icon-btn" :title="t('organismActions.close')" :aria-label="t('organismActions.close')" autofocus @click="emit('close')"><X :size="17" /></button>
    </header>
    <div ref="logBody" class="organism-log-body" aria-live="polite">
      <p v-if="error" class="organism-log-error" role="alert">{{ error }}</p>
      <pre v-if="output.length" class="organism-log-output"><span v-for="(part, index) in output" :key="index" :class="{ 'organism-log-error': part.failed }">{{ part.text }}</span></pre>
      <p v-else class="organism-log-empty">{{ loading ? t('common.loading') : reports.length ? t('organismActions.noOutput') : t('organismActions.noLogs') }}</p>
    </div>
  </dialog>
</template>

<style scoped>
.organism-log-dialog{width:min(640px,calc(100vw - 32px));max-height:calc(100dvh - 48px);margin:auto;padding:0;border:1px solid var(--line);border-radius:6px;background:#101824;color:#dbe7ef;letter-spacing:0;overflow:auto}
.organism-log-dialog::backdrop{background:#05090fc7}
.organism-log-header{display:flex;align-items:center;gap:8px;padding:16px 18px;border-bottom:1px solid var(--line)}
.organism-log-header>div{flex:1;min-width:0}.organism-log-header h3{font-size:15px;overflow-wrap:anywhere}.organism-log-header small{display:block;color:var(--muted);font-size:11px;margin-top:5px}
.organism-log-header button{width:30px;height:30px;flex:none}.organism-log-body{min-height:180px;max-height:calc(100dvh - 130px);overflow:auto;padding:16px 18px}
.organism-log-meta{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;margin-bottom:12px;font:11px monospace;color:var(--muted)}.organism-log-meta>span:first-child{color:var(--lime)}.organism-log-meta>span.failed,.organism-log-error{color:#ef9b9b}
.organism-log-output{margin:0;white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.7 monospace;tab-size:4}.organism-log-empty{color:var(--muted);font-size:12px;line-height:1.7}.organism-log-header button:focus-visible{outline:1px solid var(--cyan)}
.organism-log-entry{padding:0 0 14px;margin:0 0 14px;border-bottom:1px solid var(--line)}.organism-log-entry:last-child{border-bottom:0;margin-bottom:0}
</style>
