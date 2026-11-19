import { computed, ref } from 'vue'
import { componentApi, modelApi, playerApi, worldApi, enterWorld, enterLocalWorld, leaveWorld, createLocalWorld, apiRequest, getApiBase } from '../api/http'
import { localWorlds } from '../api/localWorlds'

const worlds = ref([])
const gameServers = ref([])
const worldMode = ref(null)
const worldsLoading = ref(false)
const tutorials = ref([])
const activeWorldId = ref('')
const wallet = ref(null)
const ground = ref(null)
const models = ref([])
const components = ref([])
const allComponents = ref([])
const loading = ref(false)
const error = ref('')
let generation = 0
let groundRequest = 0
let directoryRequest = 0
let tutorialRequest = null

export function useDataStore() {
  const activeWorld = computed(() => worlds.value.find((item) => item.id === activeWorldId.value) || null)
  // The unlocked endpoint already applies the user's unlock state; paid unlocks
  // intentionally keep default_unlocked=false in their definition.
  const unlockedComponents = computed(() => components.value)
  const lockedComponents = computed(() => {
    const unlockedIds = new Set(components.value.map((item) => item.id))
    return allComponents.value.filter((item) => !unlockedIds.has(item.id))
  })

  async function load() {
    const requestGeneration = ++generation
    loading.value = true; error.value = ''
    const results = await Promise.allSettled([playerApi.data(), modelApi.list(), componentApi.unlocked(), componentApi.all()])
    if (requestGeneration !== generation) return
    const [walletResult, modelResult, componentResult, allComponentResult] = results
    if (walletResult.status === 'fulfilled') wallet.value = walletResult.value.data
    if (modelResult.status === 'fulfilled') models.value = modelResult.value.data || []
    if (componentResult.status === 'fulfilled') components.value = componentResult.value.data || []
    if (allComponentResult.status === 'fulfilled') allComponents.value = allComponentResult.value.data || []
    const failed = results.find((item) => item.status === 'rejected')
    if (failed) error.value = failed.reason.message
    loading.value = false
  }

  function reset() {
    tutorialRequest?.abort()
    leaveWorld()
    generation += 1
    groundRequest += 1
    worlds.value = []
    gameServers.value = []
    tutorials.value = []
    worldMode.value = null
    worldsLoading.value = false
    activeWorldId.value = ''
    wallet.value = null
    models.value = []
    components.value = []
    allComponents.value = []
    ground.value = null
    window.lifePaperGround = null
    loading.value = false
    error.value = ''
  }
  function clearError() { error.value = '' }

  async function selectWorldMode(mode) {
    if (mode !== null && !['singleplayer', 'multiplayer', 'tutorial'].includes(mode)) return
    directoryRequest++
    tutorialRequest?.abort()
    closeWorld()
    worldMode.value = mode
    worlds.value = []; gameServers.value = []
    error.value = ''
    worldsLoading.value = false
    if (mode) await loadWorlds()
  }
  async function loadWorlds() {
    const requestGeneration = generation
    const mode = worldMode.value
    const request = ++directoryRequest
    if (!mode) return
    worldsLoading.value = true
    try {
      const result = mode === 'multiplayer' ? await worldApi.list() : mode === 'tutorial' ? await loadTutorialDirectory() : await localWorlds.list(mode)
      if (generation !== requestGeneration || worldMode.value !== mode || request !== directoryRequest) return
      worlds.value = result.data || []; gameServers.value = result.servers || []
      if (result.tutorials) tutorials.value = result.tutorials
    } catch (cause) { if (generation === requestGeneration && request === directoryRequest) error.value = cause.message }
    finally { if (generation === requestGeneration && request === directoryRequest) worldsLoading.value = false }
  }
  async function loadTutorialDirectory() {
    const source = getApiBase()
    const directory = await localWorlds.list('tutorial')
    let catalog
    try {
      catalog = await apiRequest('/tutorials')
      await localWorlds.catalog(source, catalog.data || [])
    } catch (cause) {
      if (cause.status !== 0) throw cause
      catalog = await localWorlds.catalog(source)
      if (!catalog.data?.length && !(directory.data || []).some(world => world.tutorial_definition)) throw cause
    }
    const entries = (catalog.data || []).slice().sort((a, b) => a.order - b.order)
    for (const world of directory.data || []) {
      if (world.tutorial_definition && !entries.some(item => item.id === world.tutorial_id)) entries.push(world.tutorial_definition)
    }
    return { tutorials: entries, data: directory.data || [] }
  }

  async function setActiveWorldStatus() {
    if (!activeWorld.value) return
    const requestGeneration = generation
    const result = activeWorld.value.status === 'active' ? await worldApi.pause(activeWorld.value.id) : await worldApi.resume(activeWorld.value.id)
    if (requestGeneration !== generation) return
    const updated = result.data
    worlds.value = worlds.value.map((item) => item.id === updated.id ? { ...item, ...updated } : item)
  }

  async function createLocal(name) {
    const mode = worldMode.value
    if (mode !== 'singleplayer') return
    await createLocalWorld(mode, name)
    if (worldMode.value === mode) await loadWorlds()
  }
  async function openTutorial(tutorial) {
    if (worldMode.value !== 'tutorial') return
    const request = directoryRequest, requestGeneration = generation
    tutorialRequest?.abort()
    const controller = new AbortController()
    tutorialRequest = controller
    try {
      const existing = worlds.value.find(world => world.tutorial_id === tutorial.id)
      if (existing) return selectWorld(existing.id)
      const downloaded = await apiRequest(`/tutorials/${encodeURIComponent(tutorial.id)}/package`, { signal: controller.signal })
      controller.signal.throwIfAborted()
      if (downloaded.data?.definition?.id !== tutorial.id) throw new Error('下载的教程包与所选教程不匹配')
      const result = await createLocalWorld('tutorial', null, downloaded.data, controller.signal)
      if (request !== directoryRequest || requestGeneration !== generation || worldMode.value !== 'tutorial') return
      worlds.value = [{ ...result.data, local_mode: 'tutorial', tutorial_id: tutorial.id, tutorial_definition: downloaded.data.definition }, ...worlds.value]
      return selectWorld(result.data.id)
    } catch (cause) { if (request === directoryRequest && requestGeneration === generation) error.value = cause.message }
  }

  function closeWorld() { tutorialRequest?.abort(); leaveWorld(); activeWorldId.value = ''; ground.value = null; groundRequest++; window.lifePaperGround = null }
  async function selectWorld(id) {
    enterWorld(id)
    activeWorldId.value = id
    ground.value = null; const request = ++groundRequest; window.lifePaperGround = null
    const requestGeneration = generation
    const isCurrent = () => request === groundRequest && requestGeneration === generation && activeWorldId.value === id
    try {
      if (worldMode.value === 'singleplayer' || worldMode.value === 'tutorial') await enterLocalWorld(id, worldMode.value)
      if (!isCurrent()) return
      const result = await worldApi.details(id)
      if (!isCurrent()) return
      worlds.value = worlds.value.map(item => item.id === id ? { ...item, ...result.data } : item)
      await loadGround(id)
    } catch (cause) { if (isCurrent()) error.value = cause.message }
  }
  async function loadGround(id) {
    const requestGeneration = generation
    const request = ++groundRequest
    const isCurrent = () => requestGeneration === generation && request === groundRequest && activeWorldId.value === id
    ground.value = null
    window.lifePaperGround = null
    try {
      const result = await worldApi.ground(id)
      if (!isCurrent()) return
      ground.value = result.data
      window.lifePaperGround = ground.value
    } catch (cause) {
      if (!isCurrent()) return
      error.value = cause.message
    }
  }
  async function unlockComponent(id) {
    const requestGeneration = generation
    const result = await componentApi.unlock(id)
    if (requestGeneration !== generation) return
    const unlocked = result.data.component
    if (!components.value.some((item) => item.id === unlocked.id)) components.value = [...components.value, unlocked]
    if (wallet.value) wallet.value = { ...wallet.value, energy_coins: result.data.energy_coins, gold_coins: result.data.gold_coins }
    return unlocked
  }
  return { worlds, gameServers, tutorials, worldMode, worldsLoading, activeWorld, activeWorldId, wallet, ground, models, components, allComponents, unlockedComponents, lockedComponents, loading, error, load, loadWorlds, selectWorldMode, openTutorial, createLocal, reset, clearError, selectWorld, closeWorld, loadGround, setActiveWorldStatus, unlockComponent }
}
