import { computed, ref } from 'vue'
import { componentApi, modelApi, playerApi, worldApi } from '../api/http'

const worlds = ref([])
const activeWorldId = ref('')
const wallet = ref(null)
const models = ref([])
const components = ref([])
const allComponents = ref([])
const loading = ref(false)
const error = ref('')
let generation = 0

export function useDataStore() {
  const activeWorld = computed(() => worlds.value.find((item) => item.id === activeWorldId.value) || worlds.value[0] || null)
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
    const results = await Promise.allSettled([worldApi.list(), playerApi.data(), modelApi.list(), componentApi.unlocked(), componentApi.all()])
    if (requestGeneration !== generation) return
    const [worldResult, walletResult, modelResult, componentResult, allComponentResult] = results
    if (worldResult.status === 'fulfilled') { worlds.value = worldResult.value.data || []; activeWorldId.value = worlds.value[0]?.id || '' }
    if (walletResult.status === 'fulfilled') wallet.value = walletResult.value.data
    if (modelResult.status === 'fulfilled') models.value = modelResult.value.data || []
    if (componentResult.status === 'fulfilled') components.value = componentResult.value.data || []
    if (allComponentResult.status === 'fulfilled') allComponents.value = allComponentResult.value.data || []
    const failed = results.find((item) => item.status === 'rejected')
    if (failed) error.value = failed.reason.message
    loading.value = false
  }

  function reset() {
    generation += 1
    worlds.value = []
    activeWorldId.value = ''
    wallet.value = null
    models.value = []
    components.value = []
    allComponents.value = []
    loading.value = false
    error.value = ''
  }
  function clearError() { error.value = '' }

  async function setActiveWorldStatus() {
    if (!activeWorld.value) return
    const result = activeWorld.value.status === 'active' ? await worldApi.pause(activeWorld.value.id) : await worldApi.resume(activeWorld.value.id)
    const updated = result.data
    worlds.value = worlds.value.map((item) => item.id === updated.id ? updated : item)
  }

  async function ensureTutorial() {
    const result = await worldApi.tutorial()
    const world = result.data
    if (!worlds.value.some((item) => item.id === world.id)) worlds.value = [world, ...worlds.value]
    activeWorldId.value = world.id
  }

  function selectWorld(id) { activeWorldId.value = id }
  async function unlockComponent(id) {
    const result = await componentApi.unlock(id)
    const unlocked = result.data.component
    if (!components.value.some((item) => item.id === unlocked.id)) components.value = [...components.value, unlocked]
    if (wallet.value) wallet.value = { ...wallet.value, energy_coins: result.data.energy_coins, gold_coins: result.data.gold_coins }
    return unlocked
  }
  return { worlds, activeWorld, activeWorldId, wallet, models, components, allComponents, unlockedComponents, lockedComponents, loading, error, load, reset, clearError, selectWorld, setActiveWorldStatus, ensureTutorial, unlockComponent }
}
