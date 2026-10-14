<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { ArrowLeft, ChevronRight, Download, File, Folder, FolderPlus, MoreHorizontal, Pencil, RefreshCw, Save, Trash2, Upload } from 'lucide-vue-next'
import { modelApi } from '../api/http'
import { useI18n } from '../i18n'
import ActionDialog from '../components/ActionDialog.vue'
import ToastMessage from '../components/ToastMessage.vue'

const props = defineProps({ model: Object })
const emit = defineEmits(['back', 'deleted'])
const entries = ref([])
const path = ref('')
const selectedFile = ref(null)
const fileContent = ref('')
const loading = ref(false)
const error = ref('')
const notice = ref('')
const saving = ref(false)
const filePicker = ref(null)
const folderPicker = ref(null)
const dialog = reactive({ open: false, title: '', message: '', inputLabel: '', inputValue: '', confirmText: '', destructive: false, resolve: null })
const { t, locale } = useI18n()
const sortedEntries = computed(() => [...entries.value].sort((a, b) => a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'folder' ? -1 : 1))
const breadcrumbs = computed(() => path.value ? path.value.split('/').filter(Boolean) : [])
function askDialog(options) { return new Promise((resolve) => { Object.assign(dialog, options, { open: true, resolve }) }) }
function cancelDialog() { dialog.open = false; dialog.resolve?.(null); dialog.resolve = null }
function confirmDialog() { const value = dialog.inputLabel ? dialog.inputValue : true; dialog.open = false; dialog.resolve?.(value); dialog.resolve = null }

async function loadFolder(nextPath = path.value) {
  loading.value = true; error.value = ''
  try { entries.value = (await modelApi.folder(props.model.id, nextPath)).data || []; path.value = nextPath; selectedFile.value = null; fileContent.value = '' }
  catch (cause) { error.value = cause.message }
  finally { loading.value = false }
}
async function openEntry(entry) { if (entry.kind === 'folder') return loadFolder(entry.path); selectedFile.value = entry; error.value = ''; try { fileContent.value = await modelApi.file(props.model.id, entry.path) } catch (cause) { error.value = cause.message } }
function parentFolder() { const parts = path.value.split('/').filter(Boolean); parts.pop(); return loadFolder(parts.join('/')) }
function chooseUpload() { filePicker.value?.click() }
function chooseFolderUpload() { folderPicker.value?.click() }
async function uploadFile(event) { const file = event.target.files?.[0]; if (!file) return; const target = path.value ? `${path.value}/${file.name}` : file.name; try { await modelApi.upload(props.model.id, target, file); notice.value = t('detail.uploaded', { path: target }); await loadFolder(path.value) } catch (cause) { error.value = cause.message } event.target.value = '' }
async function uploadFolder(event) {
  const files = Array.from(event.target.files || [])
  if (!files.length) return
  loading.value = true; error.value = ''; notice.value = ''
  try {
    const folderPaths = new Set()
    const targets = files.map((file) => {
      const relative = (file.webkitRelativePath || file.name).replaceAll('\\', '/').replace(/^\/+/, '')
      const parts = relative.split('/').filter(Boolean)
      for (let index = 1; index < parts.length; index += 1) folderPaths.add(parts.slice(0, index).join('/'))
      return { file, path: path.value ? `${path.value}/${relative}` : relative }
    })
    for (const folder of [...folderPaths].sort((a, b) => a.length - b.length)) await modelApi.createFolder(props.model.id, path.value ? `${path.value}/${folder}` : folder)
    for (const target of targets) await modelApi.upload(props.model.id, target.path, target.file)
    notice.value = t('detail.folderUploaded', { count: files.length })
    await loadFolder(path.value)
  } catch (cause) { error.value = cause.message }
  finally { loading.value = false; event.target.value = '' }
}
async function createFolder() { const name = await askDialog({ title: t('detail.folderName'), inputLabel: t('detail.folderName'), confirmText: t('common.confirm') }); if (!name?.trim()) return; const target = path.value ? `${path.value}/${name.trim()}` : name.trim(); try { await modelApi.createFolder(props.model.id, target); notice.value = t('detail.folderCreated', { path: target }); await loadFolder(path.value) } catch (cause) { error.value = cause.message } }
async function renameEntry(entry) { const name = await askDialog({ title: t('detail.renamePrompt'), inputLabel: t('detail.renamePrompt'), inputValue: entry.name, confirmText: t('common.confirm') }); if (!name?.trim() || name.trim() === entry.name) return; try { await modelApi.renameEntry(props.model.id, entry.path, name.trim()); notice.value = t('detail.renamed', { name: name.trim() }); await loadFolder(path.value) } catch (cause) { error.value = cause.message } }
async function deleteEntry(entry) { if (!await askDialog({ title: t('common.delete'), message: t('detail.entryDeleteConfirm', { name: entry.name }), confirmText: t('common.delete'), destructive: true })) return; try { await modelApi.deleteEntry(props.model.id, entry.path); if (selectedFile.value?.path === entry.path) selectedFile.value = null; notice.value = t('detail.deleted', { name: entry.name }); await loadFolder(path.value) } catch (cause) { error.value = cause.message } }
async function deleteModel() { if (!await askDialog({ title: t('detail.deleteModel'), message: t('detail.deleteConfirm', { name: props.model.name }), confirmText: t('common.delete'), destructive: true })) return; try { await modelApi.remove(props.model.id); emit('deleted') } catch (cause) { error.value = cause.message } }
async function createFile() { const name = await askDialog({ title: t('detail.fileName'), inputLabel: t('detail.fileName'), confirmText: t('common.confirm') }); if (!name?.trim()) return; const target = path.value ? `${path.value}/${name.trim()}` : name.trim(); try { await modelApi.upload(props.model.id, target, ''); notice.value = t('detail.fileCreated', { path: target }); await loadFolder(path.value) } catch (cause) { error.value = cause.message } }
async function saveFile() { if (!selectedFile.value) return; saving.value = true; try { await modelApi.upload(props.model.id, selectedFile.value.path, fileContent.value); notice.value = t('detail.saved', { path: selectedFile.value.path }) } catch (cause) { error.value = cause.message } finally { saving.value = false } }
function downloadFile() { if (!selectedFile.value) return; const blob = new Blob([fileContent.value], { type: 'text/plain;charset=utf-8' }); const url = URL.createObjectURL(blob); const link = document.createElement('a'); link.href = url; link.download = selectedFile.value.name; link.click(); URL.revokeObjectURL(url) }
onMounted(() => loadFolder(''))
</script>

<template>
  <section class="content model-detail"><ToastMessage :message="error || notice" :tone="error ? 'error' : 'info'" @close="error = ''; notice = ''" /><div class="detail-toolbar"><button class="ghost-btn" @click="emit('back')"><ArrowLeft :size="15" /> {{ t('detail.back') }}</button><div class="detail-actions"><button class="ghost-btn" @click="chooseUpload"><Upload :size="15" /> {{ t('detail.uploadFile') }}</button><button class="ghost-btn" @click="chooseFolderUpload"><FolderPlus :size="15" /> {{ t('detail.uploadFolder') }}</button><button class="ghost-btn" @click="createFile"><File :size="15" /> {{ t('detail.newFile') }}</button><button class="ghost-btn" @click="createFolder"><FolderPlus :size="15" /> {{ t('detail.newFolder') }}</button><button class="danger-btn" @click="deleteModel"><Trash2 :size="15" /> {{ t('detail.deleteModel') }}</button><input ref="filePicker" type="file" hidden @change="uploadFile" /><input ref="folderPicker" type="file" webkitdirectory directory multiple hidden @change="uploadFolder" /></div></div><div class="detail-heading"><div><span class="eyebrow cyan-text">{{ t('labels.detailEyebrow') }}</span><h2>{{ model.name }}</h2><p>{{ model.id }} · {{ t('detail.updated', { count: Object.keys(model.composition?.components || {}).length, time: new Date(model.updated_at).toLocaleString(locale === 'zh_CN' ? 'zh-CN' : 'en-US') }) }}</p></div></div><div class="model-detail-grid"><div class="file-panel"><div class="folder-bar"><button class="icon-btn" :disabled="!path" @click="parentFolder"><ArrowLeft :size="16" /></button><button class="crumb" @click="loadFolder('')">{{ t('common.root') }}</button><template v-for="(crumb, index) in breadcrumbs" :key="crumb"><ChevronRight :size="13" /><button class="crumb" @click="loadFolder(breadcrumbs.slice(0, index + 1).join('/'))">{{ crumb }}</button></template><button class="icon-btn refresh" @click="loadFolder(path)"><RefreshCw :size="15" /></button></div><div v-if="loading" class="panel-empty">{{ t('detail.loading') }}</div><div v-else-if="!sortedEntries.length" class="panel-empty">{{ t('detail.empty') }}</div><div v-else class="file-list"><div v-for="entry in sortedEntries" :key="entry.path" class="file-row" :class="{ selected: selectedFile?.path === entry.path }" @dblclick="openEntry(entry)"><button class="file-main" @click="openEntry(entry)"><Folder v-if="entry.kind === 'folder'" :size="17" class="folder-icon" /><File v-else :size="17" class="file-icon" /><span>{{ entry.name }}</span></button><small>{{ entry.kind === 'file' ? `${entry.size || 0} B` : t('detail.folder') }}</small><div class="file-actions"><button class="icon-btn" :title="t('common.rename')" @click.stop="renameEntry(entry)"><Pencil :size="14" /></button><button class="icon-btn" :title="t('common.delete')" @click.stop="deleteEntry(entry)"><Trash2 :size="14" /></button><MoreHorizontal :size="15" /></div></div></div></div><div class="code-panel"><div class="code-header"><span>{{ selectedFile?.path || t('detail.chooseFile') }}</span><div v-if="selectedFile" class="code-actions"><button class="icon-btn" :title="t('common.save')" :disabled="saving" @click="saveFile"><Save :size="15" /></button><button class="icon-btn" :title="t('detail.download')" @click="downloadFile"><Download :size="15" /></button></div></div><textarea v-if="selectedFile" v-model="fileContent" class="code-editor" spellcheck="false"></textarea><div v-else class="panel-empty">{{ t('detail.editorHint') }}</div></div></div><ActionDialog v-if="dialog.open" v-model:input-value="dialog.inputValue" v-bind="dialog" :cancel-text="t('common.cancel')" @confirm="confirmDialog" @cancel="cancelDialog" /></section>
</template>
