<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { organismApi, apiRequest } from '../api/http'
import { ArrowLeft, BookOpen, ChevronRight, Crosshair, FileText, HardDrive, LogIn, Pause, Play, Plus, RefreshCw, Users } from 'lucide-vue-next'
import WorldScene from '../components/WorldScene.vue'
import ModelDetailView from './ModelDetailView.vue'
import OrganismLogDialog from '../components/OrganismLogDialog.vue'
import SpawnOrganismDialog from '../components/SpawnOrganismDialog.vue'
import { useI18n } from '../i18n'

const props = defineProps({ worlds: Array, tutorials: Array, gameServers: Array, worldMode: { type: String, default: 'multiplayer' }, worldReady: { type: Boolean, default: true }, activeWorld: Object, ground: Object, wallet: Object, models: Array, loading: Boolean, createLocalWorld: Function, openTutorial: Function })
const emit = defineEmits(['select-mode', 'select-tutorial', 'select-world', 'close-world', 'refresh-worlds', 'toggle-world', 'notify'])
const modes = [{ id: 'singleplayer', label: '单人', icon: HardDrive }, { id: 'multiplayer', label: '多人', icon: Users }, { id: 'tutorial', label: '教程', icon: BookOpen }]
const { t } = useI18n()
const selectedWorldId = ref(null)
const editingOrganism = ref(null)
const logsOrganism = ref(null)
const worldScene = ref(null)
const instanceProgramApi = computed(() => editingOrganism.value ? organismApi.program(selectedWorldId.value, editingOrganism.value.id) : null)
const createBusy = ref(false)
const creatingWorld = ref(false), worldName = ref('')
const localMode = computed(() => props.worldMode === 'singleplayer')
const modeLabel = computed(() => modes.find(mode => mode.id === props.worldMode)?.label || '')
const showOrganisms = ref(true)
const showMatter = ref(true)
const tileHover = ref(null)
const organisms = ref([])
const addingOrganism = ref(false)
const spawnModel = ref(null)
const organismBusy = ref(false)
const organismsLoading = ref(false)
const tutorialState = ref(null)
const spawnModels = computed(() => props.worldMode === 'tutorial' ? tutorialState.value?.models || [] : props.models || [])
const openingTutorial = ref(false)
const tutorialGraph = computed(() => {
  const entries = props.tutorials || [], ranks = new Map()
  function rank(entry, visiting = new Set()) {
    if (ranks.has(entry.id)) return ranks.get(entry.id)
    if (visiting.has(entry.id)) return 0
    const path = new Set(visiting).add(entry.id)
    const parents = (entry.prerequisites || []).map(id => entries.find(item => item.id === id)).filter(Boolean)
    const value = parents.length ? 1 + Math.max(...parents.map(parent => rank(parent, path))) : 0
    ranks.set(entry.id, value)
    return value
  }
  entries.forEach(entry => rank(entry))
  const columns = new Map()
  entries.forEach(entry => { const depth = ranks.get(entry.id); columns.set(depth, [...(columns.get(depth) || []), entry]) })
  const rows = Math.max(1, ...Array.from(columns.values(), items => items.length))
  const nodes = entries.map((entry, index) => {
    const column = columns.get(ranks.get(entry.id))
    return { ...entry, index, x: ranks.get(entry.id) * 300, y: ((rows - column.length) / 2 + column.indexOf(entry)) * 154 }
  })
  const edges = nodes.flatMap(node => (node.prerequisites || []).flatMap(id => {
    const parent = nodes.find(item => item.id === id)
    if (!parent) return []
    const x = parent.x + 240, y = parent.y + 58, endY = node.y + 58
    return [{ id: `${id}-${node.id}`, path: `M${x},${y} H${x + 30} V${endY} H${node.x}` }]
  }))
  return { nodes, edges, width: Math.max(240, ...nodes.map(node => node.x + 240)), height: rows * 154 - 38 }
})
watch(() => props.activeWorld?.id, id => { selectedWorldId.value = id || null })
async function openTutorial(tutorial) {
  if (openingTutorial.value) return
  openingTutorial.value = true
  try { await props.openTutorial?.(tutorial) }
  finally { openingTutorial.value = false }
}
function tutorialLocked(tutorial) {
  if (props.worlds?.some(world => world.tutorial_id === tutorial.id)) return false
  return tutorial.prerequisites?.some(id => !props.worlds?.some(world => world.tutorial_id === id && world.tutorial_completed))
}
async function refreshTutorial(worldId, revision) {
  if (props.worldMode !== 'tutorial') return
  const result = await apiRequest(`/worlds/${worldId}/tutorial`)
  if (worldId !== selectedWorldId.value || revision !== organismRequest) return
  tutorialState.value = result.data
  const world = props.worlds?.find(world => world.id === worldId)
  if (world) world.tutorial_completed = result.data?.completed || false
}
let organismRequest = 0
let pollTimer
let polling = false
async function refreshOrganismStates() {
  const worldId = selectedWorldId.value
  const revision = organismRequest
  if (!worldId || !props.worldReady || polling || organismsLoading.value || organismBusy.value) return
  polling = true
  try {
    const result = await organismApi.list(worldId)
    await refreshTutorial(worldId, revision)
    if (worldId !== selectedWorldId.value || revision !== organismRequest) return
    for (const current of organisms.value) {
      const latest = result.data.find(item => item.id === current.id)
      if (latest) {
        current.component_states = latest.component_states
        current.ai = latest.ai
        current.position = latest.position
        current.rotation_mdeg = latest.rotation_mdeg
        current.motion = latest.motion
      }
    }
  } catch { /* Retry on the next poll without interrupting the editor. */ }
  finally { polling = false }
}
onMounted(() => { pollTimer = window.setInterval(refreshOrganismStates, 1500) })
onBeforeUnmount(() => { window.clearInterval(pollTimer); organismRequest += 1; emit('close-world') })
const selectedWorld = computed(() => props.worlds?.find(world => world.id === selectedWorldId.value) || null)
const soupTotal = computed(() => props.wallet?.bio_soup?.reduce((sum, item) => sum + item.amount, 0) || 0)

function openWorld(world) { selectedWorldId.value = world.id; emit('select-world', world.id) }
function showWorldList() { selectedWorldId.value = null; emit('close-world') }
function chooseMode(mode) { selectedWorldId.value = null; creatingWorld.value = false; emit('select-mode', mode) }
function locateOrganism(organism) {
  showOrganisms.value = true
  tileHover.value = null
  worldScene.value?.focusPosition(organism.position)
}
async function createSavedWorld() {
  if (createBusy.value) return
  createBusy.value = true
  try { await props.createLocalWorld?.(worldName.value.trim()); creatingWorld.value = false }
  catch (error) { emit('notify', error.message, 'error') }
  finally { createBusy.value = false }
}
function newLocalWorld() { worldName.value = ''; creatingWorld.value = true }
watch([selectedWorldId, () => props.worldReady], async ([worldId, ready]) => {
  editingOrganism.value = null
  logsOrganism.value = null
  const request = ++organismRequest
  organisms.value = []
  tutorialState.value = null
  addingOrganism.value = false
  spawnModel.value = null
  organismsLoading.value = !!worldId
  if (!worldId || !ready) return
  try {
    const spawn = selectedWorld.value?.tutorial_definition?.map?.spawn || props.tutorials?.find(tutorial => tutorial.map?.level_id === selectedWorld.value?.level_id)?.map?.spawn
    if (props.worldMode === 'tutorial' && spawn) worldScene.value?.focusPosition(spawn)
    const result = await organismApi.list(worldId)
    await refreshTutorial(worldId, request)
    if (request === organismRequest) organisms.value = result.data
  } catch (error) {
    if (request === organismRequest) emit('notify', error.message, 'error')
  } finally {
    if (request === organismRequest) organismsLoading.value = false
  }
})
async function addOrganism(initialContents) {
  if (organismBusy.value || organismsLoading.value || !selectedWorldId.value) return
  const worldId = selectedWorldId.value
  organismBusy.value = true
  try {
    const result = await organismApi.create(worldId, spawnModel.value.id, initialContents, props.worldMode === 'tutorial' ? spawnModel.value : undefined)
    if (selectedWorldId.value === worldId) {
      organisms.value.push(result.data)
      addingOrganism.value = false
      spawnModel.value = null
    }
  } catch (error) { emit('notify', error.message, 'error') }
  finally { organismBusy.value = false }
}
</script>

<template>
  <ModelDetailView v-if="editingOrganism" :key="editingOrganism.id" :model="editingOrganism" :program-api="instanceProgramApi" instance-mode @back="editingOrganism = null" />
  <section v-else class="content world-hub">
    <template v-if="worldMode === null">
      <div class="world-list-heading"><div><span class="eyebrow cyan-text">LIFE PAPER</span><h2>世界中枢</h2></div></div>
      <div class="world-mode-list"><button v-for="mode in modes" :key="mode.id" class="world-mode-option" @click="chooseMode(mode.id)"><component :is="mode.icon" :size="25" /><strong>{{ mode.label }}</strong><ChevronRight :size="18" /></button></div>
    </template>
    <template v-else>
    <button v-if="!selectedWorld" class="world-back-btn world-mode-back" @click="chooseMode(null)"><ArrowLeft :size="17" /><span>世界中枢</span></button>
    <div v-if="loading" class="empty-state">{{ t('world.loading') }}</div>
    <div v-else-if="worldMode === 'tutorial' && !selectedWorld" class="tutorial-tree">
      <div class="world-list-heading"><div><span class="eyebrow cyan-text">教程</span><h2>教程世界</h2></div><button class="icon-btn" title="刷新教程目录" @click="emit('refresh-worlds')"><RefreshCw :size="17" /></button></div>
      <div v-if="!tutorials?.length" class="empty-state">暂无教程</div>
      <div class="tutorial-track"><div class="tutorial-graph" :style="{ width: `${tutorialGraph.width}px`, height: `${tutorialGraph.height}px` }"><svg aria-hidden="true" class="tutorial-connections" :width="tutorialGraph.width" :height="tutorialGraph.height"><path v-for="edge in tutorialGraph.edges" :key="edge.id" :d="edge.path" /></svg><button v-for="tutorial in tutorialGraph.nodes" :key="tutorial.id" class="tutorial-node" :style="{ left: `${tutorial.x}px`, top: `${tutorial.y}px` }" :class="{ locked: tutorialLocked(tutorial) }" :disabled="openingTutorial || tutorialLocked(tutorial)" :title="tutorialLocked(tutorial) ? '需要先通关前置教程' : tutorial.title" @click="openTutorial(tutorial)"><span class="tutorial-node-number">{{ tutorial.index + 1 }}</span><span><strong>{{ tutorial.title }}</strong><small>{{ tutorial.description }}</small><small v-if="worlds.some(world => world.tutorial_id === tutorial.id && world.tutorial_completed)">已通关</small></span><ChevronRight :size="18" /></button></div></div>
    </div>
    <div v-else-if="!worlds?.length" class="empty-state world-list-empty">
      <h3>{{ worldMode === 'multiplayer' ? '暂无多人世界' : '暂无单人存档' }}</h3>
      <p v-if="worldMode === 'multiplayer' && gameServers && !gameServers.length">暂无在线世界服务器</p>
      <button v-if="localMode" class="primary-btn" :disabled="createBusy" @click="newLocalWorld"><Plus :size="15" />新建单人世界</button>
      <button class="ghost-btn" :disabled="createBusy" @click="emit('refresh-worlds')"><RefreshCw :size="15" />刷新列表</button>
    </div>
    <template v-else-if="!selectedWorld">
      <div class="world-list-heading"><div><span class="eyebrow cyan-text">{{ modeLabel }}</span><h2>{{ t('world.listTitle') }}</h2><p>{{ t('world.listSubtitle') }}</p></div><span class="world-list-count">{{ worlds.length }}</span><button v-if="localMode" class="icon-btn" title="新建本地世界" :disabled="createBusy" @click="newLocalWorld"><Plus :size="17" /></button><button class="icon-btn" title="刷新世界列表" @click="emit('refresh-worlds')"><RefreshCw :size="17" /></button></div>
      <div class="world-list-grid">
        <button v-for="world in worlds" :key="world.id" class="world-list-item" :class="{ selected: world.id === activeWorld?.id }" @click="openWorld(world)">
          <span class="world-list-icon"><i class="live-dot" :class="{ paused: world.status !== 'active' }"></i></span>
          <span class="world-list-main"><strong>{{ world.name }}</strong><small>{{ world.server_name || world.level_id }}<template v-if="world.kind"> · {{ world.kind }}</template></small><em v-if="world.status">{{ world.status === 'active' ? t('world.running') : t('world.paused') }}</em></span>
          <ChevronRight :size="18" />
        </button>
      </div>
    </template>
    <template v-else>
      <div class="world-workspace-header">
        <button class="world-back-btn" title="返回世界列表" @click="showWorldList"><ArrowLeft :size="17" /><span>{{ t('world.listTitle') }}</span></button>
        <div class="world-workspace-title"><span class="eyebrow cyan-text">{{ t('labels.worldMap') }}</span><h2>{{ selectedWorld.name }}</h2></div>
        <button v-if="selectedWorld.status" class="ghost-btn world-pause" @click="emit('toggle-world')"><Pause v-if="selectedWorld.status === 'active'" :size="14" /><Play v-else :size="14" />{{ selectedWorld.status === 'active' ? t('world.pause') : t('world.resume') }}</button>
      </div>
      <div class="world-workspace">
        <div class="world-map-panel">
          <div class="world-map-meta">
            <span>{{ selectedWorld.level_id }}</span>
            <div class="layer-options" role="group" aria-label="地图叠加层">
              <label><input v-model="showOrganisms" type="checkbox" />生物层</label>
              <label><input v-model="showMatter" type="checkbox" />汤层</label>
            </div>
            <span v-if="selectedWorld.status" class="world-map-status"><i class="live-dot" :class="{ paused: selectedWorld.status !== 'active' }"></i>{{ selectedWorld.status === 'active' ? t('world.running') : t('world.paused') }}</span>
          </div>
          <WorldScene ref="worldScene" :ground="ground" :show-organisms="showOrganisms" :show-matter="showMatter" :organisms="organisms" @tile-hover="tileHover = $event" />
          <div v-if="tileHover" class="tile-inspector"><b>({{ tileHover.x }}, {{ tileHover.y }}) · {{ tileHover.key }}</b><span>高度 {{ tileHover.height }}%　干湿 {{ tileHover.moisture }}%</span><span>营养 {{ tileHover.nutrients }}%　温度 {{ tileHover.temperature }}%</span></div>
        </div>
        <aside class="world-creature-panel">
          <section v-if="tutorialState" class="tutorial-instructions" aria-label="教程任务"><template v-for="panel in tutorialState.panels" :key="panel.event_id"><h3 v-if="panel.title">{{ panel.title }}</h3><p v-if="panel.message">{{ panel.message }}</p><ol v-if="panel.steps"><li v-for="step in panel.steps" :key="step">{{ step }}</li></ol></template><strong v-if="tutorialState.completed">已通关</strong></section>
          <div class="world-creature-heading"><div><span class="eyebrow">{{ t('world.creatureEyebrow') }}</span><h3>{{ t('world.creatures') }}</h3></div><button v-if="!localMode" class="icon-btn" title="添加生物" :disabled="organismBusy || organismsLoading" @click="addingOrganism = !addingOrganism"><Plus :size="17" /></button></div>
          <div v-if="addingOrganism" class="organism-picker"><button v-for="model in spawnModels" :key="model.id" :disabled="organismBusy || organismsLoading" @click="spawnModel = model">{{ model.name }}</button><small v-if="!spawnModels.length">{{ t('world.noCreaturesHint') }}</small></div>
          <div v-if="organismsLoading" class="world-creature-empty">{{ t('world.loading') }}</div>
          <div v-else-if="!organisms.length" class="world-creature-empty"><span class="component-glyph violet"><Plus :size="17" /></span><strong>{{ t('world.noCreatures') }}</strong><small>{{ t('world.noCreaturesHint') }}</small></div>
          <div v-else class="world-creature-list">
            <div v-for="organism in organisms" :key="organism.id" class="world-creature-row">
              <span class="component-glyph violet">{{ organism.name.slice(0, 1) }}</span>
              <span class="world-creature-identity"><strong :title="organism.name">{{ organism.name }}</strong><small>({{ organism.position.x }}, {{ organism.position.y }}) · {{ organism.rotation_mdeg / 1000 }}°</small></span>
              <div class="world-creature-actions">
                <button class="icon-btn" :title="t('organismActions.locate')" :aria-label="t('organismActions.locate')" @click="locateOrganism(organism)"><Crosshair :size="16" /></button>
                <button class="icon-btn" :title="t('organismActions.logs')" :aria-label="t('organismActions.logs')" @click="logsOrganism = organism"><FileText :size="16" /></button>
                <button class="icon-btn" :title="t('organismActions.enter')" :aria-label="t('organismActions.enter')" @click="editingOrganism = organism"><LogIn :size="16" /></button>
              </div>
            </div>
          </div>
          <div class="world-resource-line"><span>{{ t('world.soup') }}</span><strong>{{ soupTotal }}</strong></div>
        </aside>
      </div>
      <OrganismLogDialog v-if="logsOrganism" :world-id="selectedWorldId" :organism="logsOrganism" @close="logsOrganism = null" @updated="logsOrganism.ai = $event" />
      <SpawnOrganismDialog v-if="spawnModel" :key="spawnModel.id" :model="spawnModel" :busy="organismBusy" @close="spawnModel = null" @confirm="addOrganism" />
    </template>
    </template>
    <div v-if="creatingWorld" class="modal-backdrop" @click.self="!createBusy && (creatingWorld = false)"><form class="action-dialog" role="dialog" aria-modal="true" aria-label="新建单人世界" @submit.prevent="createSavedWorld"><h3>新建单人世界</h3><label class="local-world-name">世界名称<input v-model="worldName" required maxlength="128" autofocus :disabled="createBusy" /></label><div class="modal-actions"><button type="button" class="ghost-btn" :disabled="createBusy" @click="creatingWorld = false">取消</button><button class="primary-btn" :disabled="createBusy || !worldName.trim()"><Plus :size="15" />创建</button></div></form></div>
  </section>
</template>

<style>
.world-list-heading{display:flex;align-items:flex-end;justify-content:space-between;margin-bottom:24px}.world-list-heading h2{font-size:29px;letter-spacing:-.04em;margin-top:9px}.world-list-heading p{color:var(--muted);font-size:13px;margin-top:9px}.world-list-count{font:500 24px 'DM Mono',monospace;color:var(--cyan);border-bottom:1px solid rgba(89,217,230,.35);padding:0 2px 8px}.world-list-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px}.world-list-item{display:flex;align-items:center;gap:14px;min-height:92px;padding:17px 19px;border:1px solid var(--line);background:rgba(16,25,37,.78);color:#aebdcc;text-align:left}.world-list-item:hover,.world-list-item.selected{border-color:rgba(89,217,230,.55);background:rgba(31,65,78,.42);color:#e8fbff}.world-list-icon{width:32px;height:32px;display:grid;place-items:center;border:1px solid rgba(89,217,230,.25);background:rgba(89,217,230,.08)}.world-list-main{display:flex;flex:1;flex-direction:column;gap:5px}.world-list-main strong{font-size:13px}.world-list-main small,.world-list-main em{font:10px 'DM Mono',monospace;color:#6e8498;font-style:normal}.world-list-main em{color:var(--lime)}.world-workspace-header{display:flex;align-items:center;gap:24px;margin-bottom:18px}.world-back-btn{display:inline-flex;align-items:center;gap:8px;height:36px;padding:0 11px;border:1px solid var(--line);background:transparent;color:#9db0c0;font-size:11px}.world-back-btn:hover{border-color:var(--cyan);color:var(--cyan)}.world-workspace-title{flex:1}.world-workspace-title h2{font-size:24px;letter-spacing:-.03em;margin-top:5px}.world-pause{display:flex;align-items:center;gap:7px}.world-workspace{display:grid;grid-template-columns:minmax(0,1fr) 320px;gap:14px;min-height:calc(100vh - 205px)}.world-map-panel,.world-creature-panel{border:1px solid var(--line);background:rgba(14,23,35,.9)}.world-map-panel{display:flex;flex-direction:column;min-height:620px}.world-map-meta{height:45px;padding:0 16px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--line);font:10px 'DM Mono',monospace;color:#7890a5}.world-map-status{display:flex;align-items:center;gap:7px;color:#a4b6c4}.world-map-panel .world-canvas{height:auto;flex:1;min-height:0;background:#0b141d}.world-creature-panel{display:flex;flex-direction:column}.world-creature-heading{display:flex;align-items:flex-start;justify-content:space-between;padding:19px 18px 15px;border-bottom:1px solid var(--line)}.world-creature-heading h3{font-size:16px;margin-top:7px}.world-creature-empty{display:flex;flex:1;align-items:center;justify-content:center;flex-direction:column;gap:9px;padding:30px;text-align:center}.world-creature-empty strong{font-size:12px;color:#d5e2ec}.world-creature-empty small{max-width:190px;color:#71869a;font-size:10px;line-height:1.6}.world-creature-list{padding:0 14px}.world-creature-row{display:flex;align-items:center;gap:10px;padding:13px 4px;border-bottom:1px solid var(--line);color:#9fb0bf}.world-creature-row>span:nth-child(2){display:flex;flex:1;flex-direction:column;gap:4px}.world-creature-row strong{font-size:11px;color:#d7e3eb}.world-creature-row small{font:9px 'DM Mono',monospace;color:#71869a}.world-resource-line{display:flex;justify-content:space-between;padding:15px 18px;border-top:1px solid var(--line);font-size:10px;color:#7d91a4}.world-resource-line strong{font:11px 'DM Mono',monospace;color:var(--cyan)}@media(max-width:900px){.world-list-grid{grid-template-columns:1fr}.world-workspace{grid-template-columns:1fr}.world-creature-panel{min-height:260px}.world-workspace{min-height:0}}
</style>

<style>
.world-workspace{min-height:0!important;align-items:start}
.world-map-panel,.world-creature-panel{min-width:0}
.world-creature-row{width:100%;background:transparent;border:0;border-bottom:1px solid var(--line);text-align:left}.world-creature-row:hover{background:rgba(89,217,230,.04)}
.world-creature-row>.component-glyph{flex:none}.world-creature-identity{min-width:0}.world-creature-identity strong{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.world-creature-identity small{overflow-wrap:anywhere}.world-creature-actions{display:flex;gap:2px;flex:none}.world-creature-actions button{width:28px;height:30px;padding:0}.world-creature-actions button:hover,.world-creature-actions button:focus-visible{color:var(--cyan);background:rgba(89,217,230,.1);outline:1px solid rgba(89,217,230,.35)}
.world-map-panel{min-height:0!important}
.world-map-panel .world-canvas{height:min(75vh,760px)!important;flex:none!important;touch-action:none;cursor:grab}
.world-map-panel .world-canvas:active{cursor:grabbing}
.layer-options { display:flex; align-items:center; gap:16px; }
.layer-options label { display:flex; align-items:center; gap:6px; white-space:nowrap; cursor:pointer; color:#b9cbd5; font-size:11px; }
.layer-options input { width:14px; height:14px; margin:0; accent-color:var(--cyan); cursor:pointer; }
.local-world-name{display:flex;flex-direction:column;gap:8px;margin:20px 0;font-size:12px}.local-world-name input{height:38px;max-width:100%;border:1px solid var(--line);background:var(--bg);color:inherit;padding:8px}
.tutorial-track{overflow:auto;padding:28px 8px 36px}.tutorial-graph{position:relative}.tutorial-connections{position:absolute;inset:0;pointer-events:none}.tutorial-connections path{fill:none;stroke:var(--amber);stroke-width:1.5;opacity:.55}.tutorial-node{position:absolute;display:flex;align-items:center;gap:14px;width:240px;min-height:116px;padding:16px;border:1px solid var(--line);background:rgba(16,25,37,.9);color:var(--amber);text-align:left}.tutorial-node-number{width:30px;height:30px;display:grid;place-items:center;border:1px solid currentColor;border-radius:50%;font:12px 'DM Mono',monospace;flex:none}.tutorial-node span:nth-child(2){display:flex;flex:1;flex-direction:column;gap:7px;min-width:0}.tutorial-node strong{color:#f3ead5;overflow-wrap:anywhere}.tutorial-node small{color:#9da6ae;line-height:1.5}.tutorial-node.locked{opacity:.45}.tutorial-node:hover{border-color:var(--amber)}
.tutorial-tree{min-width:0;max-width:100%}.tutorial-node{flex:0 0 240px;max-width:240px;border-radius:6px}.tutorial-node:disabled{cursor:default}.tutorial-node.locked:hover{transform:none;border-color:var(--line)}.tutorial-instructions{padding:18px;border-bottom:1px solid var(--line);font-size:13px;line-height:1.7;overflow-wrap:anywhere}.tutorial-instructions h3{font-size:16px}.tutorial-instructions ol{padding-left:20px;color:var(--muted)}.tutorial-instructions strong{color:var(--lime)}
.world-mode-list{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:14px}.world-mode-option{display:flex;align-items:center;gap:16px;min-height:130px;padding:24px;border:1px solid var(--line);border-radius:4px;background:rgba(16,25,37,.78);color:var(--cyan);text-align:left}.world-mode-option strong{flex:1;min-width:0;font-size:18px;color:#e0edf6}.world-mode-option:nth-child(2){color:var(--lime)}.world-mode-option:nth-child(3){color:var(--amber)}.world-mode-option:hover{border-color:currentColor}.world-mode-back{margin-bottom:20px}@media(max-width:900px){.world-mode-list{grid-template-columns:1fr}.world-mode-option{min-height:96px;padding:20px}}
.world-list-heading{gap:12px}.world-list-heading>div{flex:1;min-width:0}.world-list-heading>.icon-btn{flex:none}.world-list-main{min-width:0}.world-list-main strong,.world-list-main small{overflow-wrap:anywhere}.world-list-empty select{max-width:100%;padding:8px;margin-bottom:14px;border:1px solid var(--line);background:var(--bg);color:inherit}.world-list-empty>.ghost-btn{display:inline-flex;align-items:center;gap:6px;margin-top:12px}
@media(max-width:700px){.world-list-heading h2{font-size:24px;letter-spacing:0}.world-list-item{padding:14px;gap:10px}.world-workspace-header{flex-wrap:wrap;gap:10px}.world-workspace-title{min-width:0}.world-workspace-title h2{overflow-wrap:anywhere;font-size:20px}.world-map-meta{height:auto;min-height:45px;flex-wrap:wrap;gap:8px;padding:8px}.layer-options{gap:8px}}
.layer-switch{display:flex;gap:3px}.layer-switch button{border:1px solid var(--line);background:#0b1621;color:#7f95a7;padding:5px 8px;font-size:10px}.layer-switch button.active{background:rgba(89,217,230,.16);border-color:rgba(89,217,230,.5);color:var(--cyan)}.tile-inspector{position:absolute;left:18px;bottom:18px;display:flex;flex-direction:column;gap:5px;padding:10px 12px;background:rgba(7,14,22,.9);border:1px solid rgba(89,217,230,.35);font:10px 'DM Mono',monospace;color:#b9cbd5;pointer-events:none}.tile-inspector b{color:#e6faff}.world-map-panel{position:relative}.organism-picker{display:flex;flex-direction:column;gap:5px;padding:12px;border-bottom:1px solid var(--line)}.organism-picker button{border:1px solid var(--line);background:#101f2c;color:#c3d4df;padding:8px;text-align:left;font-size:11px}.organism-picker small{color:#71869a;font-size:10px;line-height:1.5}
</style>
