<script setup>
import { computed, nextTick, ref, watch } from 'vue'
import { useDataStore } from '../stores/data'
import { useI18n } from '../i18n'
import { organismRenderParts } from '../rendering/organism'

const props = defineProps({ organism: { type: Object, required: true } })
const { t } = useI18n()
const catalog = useDataStore().allComponents
const definitions = computed(() => new Map(catalog.value.map(item => [item.id, item])))
const composition = computed(() => props.organism.composition || {})
const components = computed(() => Object.values(composition.value.components || {}))
const parts = computed(() => organismRenderParts(composition.value, definitions.value))
const selectedId = ref(null)
const hoveredId = ref(null)
const focusedId = ref(null)
const inspectedId = computed(() => hoveredId.value || focusedId.value || selectedId.value)
const drawing = ref(null)
const infoBox = ref(null)
const viewBox = ref('-60 -60 120 120')
const selected = computed(() => components.value.find(part => part.id === inspectedId.value))
const definition = computed(() => definitions.value.get(selected.value?.component || selected.value?.component_id))
const state = computed(() => props.organism.component_states?.[inspectedId.value])
const componentName = computed(() => definition.value?.name || selected.value?.component || selected.value?.component_id || t('models.inspector'))
const unavailable = computed(() => components.value.length - parts.value.length)
const fields = [
  ['max_durability', 'maxDurability', ''], ['damage_weight', 'damageWeight', ''],
  ['repair_weight', 'repairWeight', ''], ['mass_mg', 'mass', 'mg'],
  ['energy_capacity_mj', 'energyCapacity', 'mJ'], ['non_gas_capacity_mg', 'matterCapacity', 'mg'],
  ['gas_capacity_ul', 'gasCapacity', 'µL'], ['reaction_min_mj_per_tick', 'reactionMin', 'mJ/tick'],
  ['reaction_max_mj_per_tick', 'reactionMax', 'mJ/tick'], ['sensor_slots', 'sensorSlots', ''],
]
const info = computed(() => {
  const part = selected.value
  if (!part) return ''
  const placement = part.placement || {}
  const properties = state.value?.properties || definition.value?.properties || {}
  const size = placement.size || definition.value?.model?.size
  const container = state.value?.container
  const hasContents = key => Object.values(container?.[key] || {}).some(amount => amount > 0)
  const hasEnergy = properties.energy_capacity_mj > 0 || state.value?.energy_mj > 0
  const hasMatter = (container?.non_gas_capacity_mg ?? properties.non_gas_capacity_mg) > 0 || hasContents('non_gas_contents_mg')
  const hasGas = (container?.gas_capacity_ul ?? properties.gas_capacity_ul) > 0 || hasContents('gas_contents_ul')
  const hasReaction = properties.reaction_min_mj_per_tick !== 0 && properties.reaction_min_mj_per_tick != null
    || properties.reaction_max_mj_per_tick !== 0 && properties.reaction_max_mj_per_tick != null
  const lines = [
    `UUID: ${part.id}`,
    `${t('inspector.name')}: ${componentName.value}`,
    `${t('inspector.typeId')}: ${part.component || part.component_id}`,
  ]
  if (definition.value?.category) lines.push(`${t('inspector.category')}: ${t(`models.category.${definition.value.category}`)}`)
  if (size && definition.value?.model?.assembly !== 'wrap') {
    const dimensions = [size.width_px, size.length_px].filter(value => value > 0)
    if (dimensions.length) lines.push(`${t('models.size')}: ${dimensions.join(' × ')} px`)
  }
  if (placement.rotation_mdeg != null && definition.value?.model?.assembly !== 'interior') lines.push(`${t('models.rotation')}: ${placement.rotation_mdeg / 1000}°`)
  if (placement.host_body) lines.push(`${t('models.host')}: ${placement.host_body}`)
  if (placement.position) lines.push(`${t('models.position')}: (${placement.position.x}, ${placement.position.y})`)
  if (state.value) {
    lines.push(`${t('inspector.durability')}: ${state.value.durability}${properties.max_durability != null ? ` / ${properties.max_durability}` : ''}`)
    if (hasEnergy && state.value.energy_mj != null) lines.push(`${t('inspector.energy')}: ${state.value.energy_mj} mJ`)
  }
  for (const [key, label, unit] of fields) {
    if (state.value?.container && ['non_gas_capacity_mg', 'gas_capacity_ul'].includes(key)) continue
    if (key.startsWith('reaction_') && !hasReaction) continue
    if (!['max_durability', 'damage_weight', 'repair_weight'].includes(key) && !key.startsWith('reaction_') && !(properties[key] > 0)) continue
    if (properties[key] != null) lines.push(`${t(`inspector.${label}`)}: ${properties[key]}${unit ? ` ${unit}` : ''}`)
  }
  if (container) {
    if (hasMatter && container.non_gas_capacity_mg != null) lines.push(`${t('inspector.matterCapacity')}: ${container.non_gas_capacity_mg} mg`)
    if (hasGas && container.gas_capacity_ul != null) lines.push(`${t('inspector.gasCapacity')}: ${container.gas_capacity_ul} µL`)
    for (const [key, label, unit] of [['non_gas_contents_mg', 'matterContents', 'mg'], ['gas_contents_ul', 'gasContents', 'µL']]) {
      if (key === 'non_gas_contents_mg' ? !hasMatter : !hasGas) continue
      const contents = Object.entries(container[key] || {}).filter(([, amount]) => amount > 0).map(([id, amount]) => `${id}: ${amount} ${unit}`).join(', ')
      lines.push(`${t(`inspector.${label}`)}: ${contents || '—'}`)
    }
  }
  return lines.join('\n')
})

watch(info, async () => {
  await nextTick()
  if (infoBox.value) {
    infoBox.value.setSelectionRange(0, 0)
    infoBox.value.scrollTop = 0
  }
}, { flush: 'post' })

watch(() => props.organism.id, () => {
  selectedId.value = composition.value.root_body || components.value[0]?.id || null
  hoveredId.value = null
  focusedId.value = null
}, { immediate: true })
watch(parts, async () => {
  await nextTick()
  const bounds = drawing.value?.getBBox()
  if (!bounds || !bounds.width || !bounds.height) return
  const padding = Math.max(10, Math.max(bounds.width, bounds.height) * 0.12)
  viewBox.value = `${bounds.x - padding} ${bounds.y - padding} ${bounds.width + 2 * padding} ${bounds.height + 2 * padding}`
}, { immediate: true, flush: 'post' })
function nameOf(id) {
  const part = components.value.find(item => item.id === id)
  return definitions.value.get(part?.component || part?.component_id)?.name || part?.component || id
}
</script>

<template>
  <div class="organism-inspector">
    <div class="organism-heading"><slot name="heading" /></div>
    <div class="component-information">
      <div class="component-information-title"><strong>{{ componentName }}</strong><span>{{ t('models.inspector') }}</span></div>
      <textarea ref="infoBox" class="component-information-text" :value="info" readonly spellcheck="false" :aria-label="t('inspector.information')"></textarea>
    </div>
    <div class="organism-preview" @pointerleave="hoveredId = null">
      <svg v-if="parts.length" :viewBox="viewBox" preserveAspectRatio="xMidYMid meet" :aria-label="t('inspector.appearance')" role="group">
        <g ref="drawing">
          <g v-for="part in parts" :key="part.id" :transform="part.transform" :data-component-id="part.id" class="preview-component" :class="{ selected: inspectedId === part.id }" tabindex="0" role="button" :aria-label="nameOf(part.id)" :aria-pressed="selectedId === part.id" @pointerenter="hoveredId = part.id" @pointerleave="hoveredId = null" @focus="focusedId = part.id" @blur="focusedId = null" @click="selectedId = part.id" @keydown.enter.prevent="selectedId = part.id" @keydown.space.prevent="selectedId = part.id">
            <title>{{ nameOf(part.id) }} · {{ part.id }}</title>
            <path :d="part.d" :fill="part.fill" :stroke="part.stroke" :stroke-width="part.strokeWidth" class="preview-solid" />
            <path :d="part.d" :fill="part.fill === 'none' ? 'none' : 'transparent'" stroke="transparent" stroke-width="10" vector-effect="non-scaling-stroke" :pointer-events="part.fill === 'none' ? 'stroke' : 'all'" />
          </g>
        </g>
      </svg>
      <span v-else class="preview-empty">{{ t('inspector.noAppearance') }}</span>
      <small v-if="unavailable" class="preview-unavailable">{{ t('inspector.unavailable', { count: unavailable }) }}</small>
    </div>
  </div>
</template>

<style scoped>
.organism-inspector{display:grid;grid-template-columns:minmax(180px,1fr) minmax(180px,280px) minmax(200px,280px);gap:16px;align-items:start;margin-bottom:22px;min-width:0;letter-spacing:0}
.organism-heading{min-width:0;padding-top:8px}
.organism-heading :deep(.detail-heading){margin:0}
.organism-heading :deep(h2){font-size:24px;letter-spacing:0;overflow-wrap:anywhere}
.organism-heading :deep(p){line-height:1.7;overflow-wrap:anywhere}
.component-information{grid-column:2;width:100%;max-width:280px;min-width:0;justify-self:end;overflow:hidden;border:1px solid var(--line);background:#0b121a}
.component-information-title{height:35px;padding:0 12px;display:flex;align-items:center;justify-content:space-between;gap:12px;border-bottom:1px solid var(--line);font-size:11px;min-width:0}
.component-information-title strong{color:#d9e8df;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}.component-information-title span{color:var(--muted);white-space:nowrap;font-size:10px}
.component-information-text{display:block;width:100%;height:203px;margin:0;border:0;resize:none;outline:none;padding:10px 12px;background:transparent;color:#c3ced8;font:11px/1.7 'DM Mono',monospace;white-space:pre-wrap;overflow-wrap:anywhere;word-break:break-word}
.component-information-text:focus-visible{box-shadow:inset 0 0 0 1px var(--cyan)}
.organism-preview{grid-column:3;position:relative;display:grid;place-items:center;min-width:0;height:240px;background:#0b121a;border:1px solid var(--line);overflow:hidden}
.organism-preview svg{display:block;width:100%;height:100%;padding:10px}
.preview-component{cursor:pointer;outline:none}.preview-solid{pointer-events:none}.preview-component.selected .preview-solid{stroke:#e8c676}.preview-component:focus-visible .preview-solid{stroke:#e8c676;filter:brightness(1.4)}
.preview-empty,.preview-unavailable{font-size:11px;color:var(--muted);text-align:center;padding:10px}.preview-unavailable{position:absolute;bottom:0;background:#0b121ae8;width:100%}
@media(max-width:900px){.organism-inspector{grid-template-columns:minmax(0,1fr) minmax(180px,240px);gap:12px}.organism-heading{grid-column:1 / -1}.component-information{grid-column:1;grid-row:2}.organism-preview{grid-column:2;grid-row:2;height:240px}}
@media(max-width:520px){.organism-inspector{grid-template-columns:minmax(0,1fr)}.organism-preview{grid-column:1;grid-row:3;height:210px}.component-information{grid-column:1;grid-row:2;max-width:none}.component-information-title{gap:8px}}
</style>
