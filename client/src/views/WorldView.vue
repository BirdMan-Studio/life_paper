<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { organismApi } from '../api/http'
import { ArrowLeft, ChevronRight, Crosshair, FileText, LogIn, Pause, Play, Plus, RefreshCw } from 'lucide-vue-next'
import WorldScene from '../components/WorldScene.vue'
import ModelDetailView from './ModelDetailView.vue'
import OrganismLogDialog from '../components/OrganismLogDialog.vue'
import { useI18n } from '../i18n'

const props = defineProps({ worlds: Array, activeWorld: Object, ground: Object, wallet: Object, models: Array, loading: Boolean })
const emit = defineEmits(['select-world', 'toggle-world', 'tutorial', 'notify'])
const { t } = useI18n()
const selectedWorldId = ref(null)
const editingOrganism = ref(null)
const logsOrganism = ref(null)
const worldScene = ref(null)
const instanceProgramApi = computed(() => editingOrganism.value ? organismApi.program(selectedWorldId.value, editingOrganism.value.id) : null)
const createBusy = ref(false)
const showOrganisms = ref(true)
const showMatter = ref(true)
const tileHover = ref(null)
const organisms = ref([])
const addingOrganism = ref(false)
const organismBusy = ref(false)
const organismsLoading = ref(false)
let organismRequest = 0
let pollTimer
let polling = false
async function refreshOrganismStates() {
  const worldId = selectedWorldId.value
  const revision = organismRequest
  if (!worldId || polling || organismsLoading.value || organismBusy.value) return
  polling = true
  try {
    const result = await organismApi.list(worldId)
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
onBeforeUnmount(() => { window.clearInterval(pollTimer); organismRequest += 1 })
const selectedWorld = computed(() => props.worlds?.find(world => world.id === selectedWorldId.value) || null)
const soupTotal = computed(() => props.wallet?.bio_soup?.reduce((sum, item) => sum + item.amount, 0) || 0)

function openWorld(world) { selectedWorldId.value = world.id; emit('select-world', world.id) }
function showWorldList() { selectedWorldId.value = null }
function locateOrganism(organism) {
  showOrganisms.value = true
  tileHover.value = null
  worldScene.value?.focusPosition(organism.position)
}
async function createTutorial() { createBusy.value = true; emit('tutorial'); createBusy.value = false }
watch(selectedWorldId, async (worldId) => {
  editingOrganism.value = null
  logsOrganism.value = null
  const request = ++organismRequest
  organisms.value = []
  addingOrganism.value = false
  organismsLoading.value = !!worldId
  if (!worldId) return
  try {
    const result = await organismApi.list(worldId)
    if (request === organismRequest) organisms.value = result.data
  } catch (error) {
    if (request === organismRequest) emit('notify', error.message, 'error')
  } finally {
    if (request === organismRequest) organismsLoading.value = false
  }
})
async function addOrganism(model) {
  if (organismBusy.value || organismsLoading.value || !selectedWorldId.value) return
  const worldId = selectedWorldId.value
  organismBusy.value = true
  try {
    const result = await organismApi.create(worldId, model.id)
    if (selectedWorldId.value === worldId) {
      organisms.value.push(result.data)
      addingOrganism.value = false
    }
  } catch (error) { emit('notify', error.message, 'error') }
  finally { organismBusy.value = false }
}
</script>

<template>
  <ModelDetailView v-if="editingOrganism" :key="editingOrganism.id" :model="editingOrganism" :program-api="instanceProgramApi" instance-mode @back="editingOrganism = null" />
  <section v-else class="content world-hub">
    <div v-if="loading" class="empty-state">{{ t('world.loading') }}</div>
    <div v-else-if="!worlds?.length" class="empty-state world-list-empty">
      <h3>{{ t('world.empty') }}</h3><p>{{ t('world.emptyHint') }}</p>
      <button class="primary-btn" :disabled="createBusy" @click="createTutorial"><RefreshCw :size="15" />{{ t('world.tutorial') }}</button>
    </div>
    <template v-else-if="!selectedWorld">
      <div class="world-list-heading"><div><span class="eyebrow cyan-text">{{ t('labels.worldEyebrow') }}</span><h2>{{ t('world.listTitle') }}</h2><p>{{ t('world.listSubtitle') }}</p></div><span class="world-list-count">{{ worlds.length }}</span></div>
      <div class="world-list-grid">
        <button v-for="world in worlds" :key="world.id" class="world-list-item" :class="{ selected: world.id === activeWorld?.id }" @click="openWorld(world)">
          <span class="world-list-icon"><i class="live-dot" :class="{ paused: world.status !== 'active' }"></i></span>
          <span class="world-list-main"><strong>{{ world.name }}</strong><small>{{ world.level_id }} · {{ world.kind }}</small><em>{{ world.status === 'active' ? t('world.running') : t('world.paused') }}</em></span>
          <ChevronRight :size="18" />
        </button>
      </div>
    </template>
    <template v-else>
      <div class="world-workspace-header">
        <button class="world-back-btn" title="返回世界列表" @click="showWorldList"><ArrowLeft :size="17" /><span>{{ t('world.listTitle') }}</span></button>
        <div class="world-workspace-title"><span class="eyebrow cyan-text">{{ t('labels.worldMap') }}</span><h2>{{ selectedWorld.name }}</h2></div>
        <button class="ghost-btn world-pause" @click="emit('toggle-world')"><Pause v-if="selectedWorld.status === 'active'" :size="14" /><Play v-else :size="14" />{{ selectedWorld.status === 'active' ? t('world.pause') : t('world.resume') }}</button>
      </div>
      <div class="world-workspace">
        <div class="world-map-panel">
          <div class="world-map-meta">
            <span>{{ selectedWorld.level_id }}</span>
            <div class="layer-options" role="group" aria-label="地图叠加层">
              <label><input v-model="showOrganisms" type="checkbox" />生物层</label>
              <label><input v-model="showMatter" type="checkbox" />汤层</label>
            </div>
            <span class="world-map-status"><i class="live-dot" :class="{ paused: selectedWorld.status !== 'active' }"></i>{{ selectedWorld.status === 'active' ? t('world.running') : t('world.paused') }}</span>
          </div>
          <WorldScene ref="worldScene" :ground="ground" :show-organisms="showOrganisms" :show-matter="showMatter" :organisms="organisms" @tile-hover="tileHover = $event" />
          <div v-if="tileHover" class="tile-inspector"><b>({{ tileHover.x }}, {{ tileHover.y }}) · {{ tileHover.key }}</b><span>高度 {{ tileHover.height }}%　干湿 {{ tileHover.moisture }}%</span><span>营养 {{ tileHover.nutrients }}%　温度 {{ tileHover.temperature }}%</span></div>
        </div>
        <aside class="world-creature-panel">
          <div class="world-creature-heading"><div><span class="eyebrow">{{ t('world.creatureEyebrow') }}</span><h3>{{ t('world.creatures') }}</h3></div><button class="icon-btn" title="添加生物" :disabled="organismBusy || organismsLoading" @click="addingOrganism = !addingOrganism"><Plus :size="17" /></button></div>
          <div v-if="addingOrganism" class="organism-picker"><button v-for="model in models" :key="model.id" :disabled="organismBusy || organismsLoading" @click="addOrganism(model)">{{ model.name }}</button><small v-if="!models?.length">{{ t('world.noCreaturesHint') }}</small></div>
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
    </template>
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
.layer-switch{display:flex;gap:3px}.layer-switch button{border:1px solid var(--line);background:#0b1621;color:#7f95a7;padding:5px 8px;font-size:10px}.layer-switch button.active{background:rgba(89,217,230,.16);border-color:rgba(89,217,230,.5);color:var(--cyan)}.tile-inspector{position:absolute;left:18px;bottom:18px;display:flex;flex-direction:column;gap:5px;padding:10px 12px;background:rgba(7,14,22,.9);border:1px solid rgba(89,217,230,.35);font:10px 'DM Mono',monospace;color:#b9cbd5;pointer-events:none}.tile-inspector b{color:#e6faff}.world-map-panel{position:relative}.organism-picker{display:flex;flex-direction:column;gap:5px;padding:12px;border-bottom:1px solid var(--line)}.organism-picker button{border:1px solid var(--line);background:#101f2c;color:#c3d4df;padding:8px;text-align:left;font-size:11px}.organism-picker small{color:#71869a;font-size:10px;line-height:1.5}
</style>
