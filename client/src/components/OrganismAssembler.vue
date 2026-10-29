<script setup>
import { computed, ref, watch } from 'vue'
import { Box, Hand, Link2, Move, MousePointer2, RotateCcw, Trash2, Unlink, ZoomIn, ZoomOut } from 'lucide-vue-next'
import { useI18n } from '../i18n'

const props = defineProps({ components: { type: Array, default: () => [] } })
const { t } = useI18n()
const canvas = ref(null)
const nodes = ref([])
const connections = ref([])
const selectedId = ref('')
const hoveredId = ref('')
const linkingId = ref('')
const selectedConnection = ref('')
const zoom = ref(1)
const interactionMode = ref('select')
const pan = ref({ x: 0, y: 0 })
const draggingId = ref('')
let drag = null
const DISPLAY_SCALE = 2
const ORIGIN = { x: 360, y: 215 }
const MIN_ZOOM = 0.5
const MAX_ZOOM = 2
const ZOOM_STEP = 0.1

const definitions = computed(() => new Map(props.components.map((item) => [item.id, item])))
const nodeMap = computed(() => new Map(nodes.value.map((item) => [item.id, item])))
const selectedNode = computed(() => nodeMap.value.get(selectedId.value))
const selectedDefinition = computed(() => definitionOf(selectedNode.value))
const selectedSize = computed(() => selectedNode.value ? componentSize(selectedNode.value) : null)
const selectedHost = computed(() => definitionOf(bodyFor(selectedNode.value)))
const selectedSlots = computed(() => Object.entries(slotsOf(selectedDefinition.value)))
const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`)
const sceneStyle = computed(() => ({ transform: `translate(${pan.value.x}px, ${pan.value.y}px) scale(${zoom.value})` }))
const externalNodes = computed(() => nodes.value
  .filter((node) => categoryOf(node) === 'external_organ')
  .sort((first, second) => Number(assemblyOf(second) === 'wrap') - Number(assemblyOf(first) === 'wrap')))
const solidNodes = computed(() => nodes.value.filter((node) => categoryOf(node) !== 'external_organ'))

function slotsOf(component) { return component?.slots?.counts || component?.slots || {} }
function definitionOf(node) { return definitions.value.get(node?.componentId) }
function categoryOf(node) { return definitionOf(node)?.category }
function modelOf(node) { return definitionOf(node)?.model }
function assemblyOf(node) { return modelOf(node)?.assembly }
function appearanceOf(node) { return modelOf(node)?.appearance }
function surfaceOffset(node) { return (appearanceOf(node)?.protrusion_px || 0) * DISPLAY_SCALE }
function nodeState(node) {
  return { selected: selectedId.value === node.id, hovered: hoveredId.value === node.id,
    dragging: draggingId.value === node.id, linking: linkingId.value === node.id,
    compatible: isCompatibleTarget(node.id) }
}
function labelStyle(node) {
  const height = categoryOf(node) === 'external_organ' && assemblyOf(node) !== 'wrap' ? 0
    : assemblyOf(node) === 'wrap' && bodyFor(node) ? displaySize(bodyFor(node)).height : displaySize(node).height
  const width = canvas.value?.clientWidth || 1000
  const canvasHeight = canvas.value?.clientHeight || 600
  const labelWidth = 132
  const labelHeight = 36
  const left = Math.max(labelWidth / 2 + 4, Math.min(width - labelWidth / 2 - 4, node.x))
  const below = node.y + height / 2 + 10
  const top = below + labelHeight <= canvasHeight - 4 ? below : Math.max(4, node.y - height / 2 - labelHeight - 10)
  return { left: `${left}px`, top: `${top}px` }
}
function componentSize(node) {
  const body = bodyFor(node)
  if (assemblyOf(node) === 'wrap' && body) {
    const bodySize = componentSize(body)
    return { width: 0, length: Math.round(2 * (bodySize.length - bodySize.width) + Math.PI * bodySize.width) }
  }
  const size = modelOf(node)?.size
  return { width: size?.width_px || 0, length: size?.length_px || 0 }
}
function displaySize(node) {
  const size = componentSize(node)
  return { width: size.length * DISPLAY_SCALE, height: categoryOf(node) === 'external_organ' ? 2 : size.width * DISPLAY_SCALE }
}
function nodeStyle(node) {
  const size = displaySize(node)
  return {
    left: `${node.x}px`, top: `${node.y}px`,
    '--part-width': `${size.width}px`, '--part-height': `${size.height}px`,
    '--part-rotation': `${node.rotationMdeg / 1000}deg`,
    '--part-fill': appearanceOf(node)?.fill,
    '--part-stroke': appearanceOf(node)?.stroke,
    '--part-stroke-width': `${(appearanceOf(node)?.stroke_width_px || 1) * DISPLAY_SCALE}px`,
    '--part-radius': modelOf(node)?.geometry === 'rectangle' ? '4px' : '999px',
  }
}
function connectionKey(first, second) { return [first, second].sort().join(':') }
function connectionsFor(id, category) {
  return connections.value.filter((edge) => {
    if (edge.first !== id && edge.second !== id) return false
    const otherId = edge.first === id ? edge.second : edge.first
    return definitionOf(nodeMap.value.get(otherId))?.category === category
  }).length
}
function canConnect(firstId, secondId) {
  if (!firstId || !secondId || firstId === secondId) return false
  if (connections.value.some((edge) => connectionKey(edge.first, edge.second) === connectionKey(firstId, secondId))) return false
  const first = nodeMap.value.get(firstId)
  const second = nodeMap.value.get(secondId)
  const firstDefinition = definitionOf(first)
  const secondDefinition = definitionOf(second)
  if (!firstDefinition || !secondDefinition) return false
  return connectionsFor(firstId, secondDefinition.category) < (slotsOf(firstDefinition)[secondDefinition.category] || 0)
    && connectionsFor(secondId, firstDefinition.category) < (slotsOf(secondDefinition)[firstDefinition.category] || 0)
}
function reset() {
  drag = null
  nodes.value = []
  connections.value = []
  selectedId.value = ''
  hoveredId.value = ''
  linkingId.value = ''
  selectedConnection.value = ''
  zoom.value = 1
  pan.value = { x: 0, y: 0 }
  interactionMode.value = 'select'
  draggingId.value = ''
}
watch(() => props.components, reset, { immediate: true })

function addComponent(componentId) {
  const definition = definitions.value.get(componentId)
  if (!definition?.model) return
  const index = nodes.value.length
  const isBody = definition.category === 'body'
  const node = { id: crypto.randomUUID(), componentId, x: isBody ? ORIGIN.x + (index % 2) * 150 : 100 + (index % 4) * 145, y: isBody ? ORIGIN.y + (index % 2) * 100 : 90 + (Math.floor(index / 4) % 3) * 105, rotationMdeg: 0, root: isBody && !nodes.value.some((item) => categoryOf(item) === 'body'), hostBody: null, anchorPpm: null }
  nodes.value.push(node)
  const parent = nodes.value.find((candidate) => candidate.id !== node.id && canConnect(candidate.id, node.id))
  if (parent) {
    connections.value.push({ first: parent.id, second: node.id })
    assignHost(node, parent)
    assignHost(parent, node)
  }
  selectedId.value = node.id
  selectedConnection.value = ''
}
function selectNode(id) {
  if (linkingId.value) {
    if (canConnect(linkingId.value, id)) {
      const source = nodeMap.value.get(linkingId.value)
      const target = nodeMap.value.get(id)
      connections.value.push({ first: linkingId.value, second: id })
      assignHost(source, target)
      assignHost(target, source)
    }
    linkingId.value = ''
  }
  selectedId.value = id
  selectedConnection.value = ''
}
function beginLink() { if (selectedId.value) linkingId.value = selectedId.value }
function removeSelected() {
  const node = selectedNode.value
  if (!node) return
  if (hoveredId.value === node.id) hoveredId.value = ''
  drag = null
  draggingId.value = ''
  selectedConnection.value = ''
  if (categoryOf(node) === 'body') {
    nodes.value.filter((item) => item.hostBody === node.id).forEach((part) => { part.hostBody = null; part.anchorPpm = null })
  }
  nodes.value = nodes.value.filter((item) => item.id !== node.id)
  connections.value = connections.value.filter((edge) => edge.first !== node.id && edge.second !== node.id)
  if (node.root) {
    const nextBody = nodes.value.find((item) => categoryOf(item) === 'body')
    if (nextBody) nextBody.root = true
  }
  selectedId.value = nodes.value.find((item) => item.root)?.id || ''
  linkingId.value = ''
}
function removeConnection() {
  if (!selectedConnection.value) return
  const removed = connections.value.find((edge) => connectionKey(edge.first, edge.second) === selectedConnection.value)
  connections.value = connections.value.filter((edge) => connectionKey(edge.first, edge.second) !== selectedConnection.value)
  if (removed) {
    for (const [partId, bodyId] of [[removed.first, removed.second], [removed.second, removed.first]]) {
      const part = nodeMap.value.get(partId)
      if (part?.hostBody === bodyId) { part.hostBody = null; part.anchorPpm = null }
    }
  }
  selectedConnection.value = ''
}
function bodyFor(node) { return nodeMap.value.get(node?.hostBody) }
function availableSurfaceAnchor(part, body) {
  const occupied = nodes.value
    .filter((item) => item.id !== part.id && item.hostBody === body.id && assemblyOf(item) === 'surface' && item.anchorPpm != null)
    .map((item) => item.anchorPpm % 1_000_000)
    .sort((first, second) => first - second)
  if (!occupied.length) return 500_000
  let largestGap = -1
  let anchor = 0
  for (let index = 0; index < occupied.length; index += 1) {
    const start = occupied[index]
    const end = occupied[(index + 1) % occupied.length] + (index === occupied.length - 1 ? 1_000_000 : 0)
    if (end - start > largestGap) {
      largestGap = end - start
      anchor = Math.round((start + end) / 2) % 1_000_000
    }
  }
  return anchor
}
function assignHost(part, candidateBody) {
  if (!part || categoryOf(part) === 'body' || categoryOf(candidateBody) !== 'body') return
  part.hostBody = candidateBody.id
  if (assemblyOf(part) === 'wrap') {
    part.anchorPpm = null
    part.x = candidateBody.x
    part.y = candidateBody.y
    part.rotationMdeg = candidateBody.rotationMdeg
  } else if (assemblyOf(part) === 'surface') {
    part.anchorPpm = availableSurfaceAnchor(part, candidateBody)
    applySurfaceAnchor(part, candidateBody)
  } else {
    const siblings = nodes.value.filter((item) => item.id !== part.id && item.hostBody === candidateBody.id && categoryOf(item) === 'internal_organ').length
    part.x = candidateBody.x - 24 + (siblings % 3) * 24
    part.y = candidateBody.y + (Math.floor(siblings / 3) % 2) * 22 - 11
  }
}
function connectionCountWithout(id, category, ignoredKey) {
  return connections.value.filter((edge) => {
    if (connectionKey(edge.first, edge.second) === ignoredKey || (edge.first !== id && edge.second !== id)) return false
    const otherId = edge.first === id ? edge.second : edge.first
    return categoryOf(nodeMap.value.get(otherId)) === category
  }).length
}
function canAttachToBody(node, body) {
  if (!node || !body || categoryOf(node) === 'body' || categoryOf(body) !== 'body') return false
  const existing = connections.value.find((edge) => connectionKey(edge.first, edge.second) === connectionKey(node.id, body.id))
  if (existing) return true
  const oldKey = node.hostBody ? connectionKey(node.id, node.hostBody) : ''
  const nodeCapacity = slotsOf(definitionOf(node)).body || 0
  const bodyCapacity = slotsOf(definitionOf(body))[categoryOf(node)] || 0
  return connectionCountWithout(node.id, 'body', oldKey) < nodeCapacity
    && connectionCountWithout(body.id, categoryOf(node), oldKey) < bodyCapacity
}
function distanceToBody(node, body) {
  if (assemblyOf(node) === 'interior' || assemblyOf(node) === 'wrap') return (body.x - node.x) ** 2 + (body.y - node.y) ** 2
  let nearest = Infinity
  for (let step = 0; step < 128; step += 1) {
    const point = capsulePoint(body, Math.round(step / 128 * 1_000_000))
    nearest = Math.min(nearest, (point.x - node.x) ** 2 + (point.y - node.y) ** 2)
  }
  return nearest
}
function nearestCompatibleBody(node) {
  return nodes.value
    .filter((candidate) => canAttachToBody(node, candidate))
    .map((body) => ({ body, distance: distanceToBody(node, body) }))
    .sort((first, second) => first.distance - second.distance)[0]?.body
}
function moveHost(node, body) {
  if (!node || !body || node.hostBody === body.id) return
  const previousBody = node.hostBody
  const previousEdge = previousBody && connections.value.find((edge) => connectionKey(edge.first, edge.second) === connectionKey(node.id, previousBody))
  if (previousEdge) connections.value = connections.value.filter((edge) => edge !== previousEdge)
  if (!connections.value.some((edge) => connectionKey(edge.first, edge.second) === connectionKey(node.id, body.id))) {
    if (!canConnect(node.id, body.id)) {
      if (previousEdge) connections.value.push(previousEdge)
      return
    }
    connections.value.push({ first: node.id, second: body.id })
  }
  assignHost(node, body)
}
function capsulePoint(body, perimeterPpm) {
  const point = capsuleLocalPoint(body, perimeterPpm)
  const radians = body.rotationMdeg / 1000 * Math.PI / 180
  return { x: body.x + point.x * Math.cos(radians) - point.y * Math.sin(radians), y: body.y + point.x * Math.sin(radians) + point.y * Math.cos(radians), angle: point.angle + body.rotationMdeg / 1000 }
}
function capsuleLocalPoint(body, perimeterPpm) {
  const { width, height } = displaySize(body)
  const radius = height / 2
  const straight = width - height
  const perimeter = 2 * straight + 2 * Math.PI * radius
  let distance = (perimeterPpm / 1_000_000) * perimeter
  if (distance <= straight) return { x: -straight / 2 + distance, y: -radius, angle: -90 }
  distance -= straight
  if (distance <= Math.PI * radius) {
    const angle = -90 + (distance / (Math.PI * radius)) * 180
    return { x: straight / 2 + Math.cos(angle * Math.PI / 180) * radius, y: Math.sin(angle * Math.PI / 180) * radius, angle }
  }
  distance -= Math.PI * radius
  if (distance <= straight) return { x: straight / 2 - distance, y: radius, angle: 90 }
  distance -= straight
  const angle = 90 + (distance / (Math.PI * radius)) * 180
  return { x: -straight / 2 + Math.cos(angle * Math.PI / 180) * radius, y: Math.sin(angle * Math.PI / 180) * radius, angle }
}
function applySurfaceAnchor(node, body) {
  if (node.anchorPpm == null) return
  const point = capsulePoint(body, node.anchorPpm)
  node.x = point.x
  node.y = point.y
  node.rotationMdeg = Math.round(point.angle * 1000)
}
function externalPath(node, offset = 0) {
  const body = bodyFor(node)
  if (!body) {
    return `M ${node.x - componentSize(node).length * DISPLAY_SCALE / 2} ${node.y} h ${componentSize(node).length * DISPLAY_SCALE}`
  }
  const size = displaySize(body)
  const perimeter = 2 * (size.width - size.height) + Math.PI * size.height
  const skin = assemblyOf(node) === 'wrap'
  const anchor = skin ? 0 : node.anchorPpm ?? 0
  const center = skin ? body : capsulePoint(body, anchor)
  const span = skin ? perimeter : componentSize(node).length * DISPLAY_SCALE
  const steps = Math.max(8, Math.ceil(span / 2))
  const points = Array.from({ length: steps + 1 }, (_, index) => {
    const distance = skin ? index / steps * span : (index / steps - 0.5) * span
    const ppm = ((anchor + distance / perimeter * 1_000_000) % 1_000_000 + 1_000_000) % 1_000_000
    const point = capsulePoint(body, ppm)
    const angle = point.angle * Math.PI / 180
    return `${point.x + node.x - center.x + Math.cos(angle) * offset} ${point.y + node.y - center.y + Math.sin(angle) * offset}`
  })
  return `M ${points.join(' L ')}${skin ? ' Z' : ''}`
}
function snapToHost(node) {
  if (!node || categoryOf(node) === 'body') return
  const nearestBody = nearestCompatibleBody(node)
  if (nearestBody) moveHost(node, nearestBody)
  const body = bodyFor(node)
  if (!body) return
  if (assemblyOf(node) === 'wrap') {
    node.x = body.x
    node.y = body.y
    node.rotationMdeg = body.rotationMdeg
  } else if (assemblyOf(node) === 'surface') {
    let best = { distance: Infinity, ppm: 0 }
    for (let step = 0; step < 256; step += 1) {
      const ppm = Math.round(step / 256 * 1_000_000)
      const point = capsulePoint(body, ppm)
      const distance = (point.x - node.x) ** 2 + (point.y - node.y) ** 2
      if (distance < best.distance) best = { distance, ppm }
    }
    node.anchorPpm = best.ppm
    applySurfaceAnchor(node, body)
  } else if (assemblyOf(node) === 'interior') {
    const bodySize = displaySize(body)
    const organRadius = Math.max(displaySize(node).width, displaySize(node).height) / 2
    const radius = Math.max(0, bodySize.height / 2 - organRadius)
    const capCenter = Math.max(0, (bodySize.width - bodySize.height) / 2)
    let localX = node.x - body.x
    let localY = node.y - body.y
    localY = Math.max(-radius, Math.min(radius, localY))
    if (localX > capCenter) {
      const dx = localX - capCenter
      const length = Math.hypot(dx, localY)
      if (length > radius) { localX = capCenter + dx / length * radius; localY = localY / length * radius }
    } else if (localX < -capCenter) {
      const dx = localX + capCenter
      const length = Math.hypot(dx, localY)
      if (length > radius) { localX = -capCenter + dx / length * radius; localY = localY / length * radius }
    }
    node.x = Math.round(body.x + localX)
    node.y = Math.round(body.y + localY)
  }
}
function canvasPoint(event) {
  const bounds = canvas.value.getBoundingClientRect()
  return {
    x: (event.clientX - bounds.left - bounds.width / 2 - pan.value.x) / zoom.value + bounds.width / 2,
    y: (event.clientY - bounds.top - bounds.height / 2 - pan.value.y) / zoom.value + bounds.height / 2,
    width: bounds.width,
    height: bounds.height,
  }
}
function setZoom(nextZoom) { zoom.value = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(nextZoom * 10) / 10)) }
function zoomBy(delta) { setZoom(zoom.value + delta) }
function wheelZoom(event) { zoomBy(event.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP) }
function setInteractionMode(mode) { interactionMode.value = mode; drag = null; draggingId.value = '' }
function beginPan(event) {
  drag = { type: 'pan', startX: event.clientX, startY: event.clientY, panX: pan.value.x, panY: pan.value.y }
  event.currentTarget.setPointerCapture?.(event.pointerId)
}
function pointerDownCanvas(event) {
  if (interactionMode.value === 'pan') beginPan(event)
  else if (!linkingId.value) { selectedId.value = ''; selectedConnection.value = '' }
}
function pointerDown(event, node) {
  if (linkingId.value) { selectNode(node.id); return }
  if (interactionMode.value === 'pan') { beginPan(event); return }
  if (interactionMode.value !== 'move-component') { selectNode(node.id); return }
  const point = canvasPoint(event)
  drag = { type: 'component', id: node.id, dx: point.x - node.x, dy: point.y - node.y, lastX: node.x, lastY: node.y }
  draggingId.value = node.id
  selectNode(node.id)
  event.currentTarget.setPointerCapture(event.pointerId)
}
function pointerMove(event) {
  if (!drag) return
  if (drag.type === 'pan') {
    pan.value = { x: drag.panX + event.clientX - drag.startX, y: drag.panY + event.clientY - drag.startY }
    return
  }
  const point = canvasPoint(event)
  const node = nodeMap.value.get(drag.id)
  if (!node) return
  const nextX = Math.round(Math.max(20, Math.min(point.width - 20, point.x - drag.dx)))
  const nextY = Math.round(Math.max(20, Math.min(point.height - 20, point.y - drag.dy)))
  const deltaX = nextX - node.x
  const deltaY = nextY - node.y
  node.x = nextX
  node.y = nextY
  if (categoryOf(node) === 'body') {
    nodes.value.filter((item) => item.hostBody === node.id).forEach((part) => { part.x += deltaX; part.y += deltaY })
  }
}
function pointerUp() {
  if (drag?.type === 'component') snapToHost(nodeMap.value.get(drag.id))
  drag = null
  draggingId.value = ''
}
function edgeLine(edge) {
  const first = nodeMap.value.get(edge.first)
  const second = nodeMap.value.get(edge.second)
  return first && second ? { x1: first.x, y1: first.y, x2: second.x, y2: second.y } : null
}
function isEdgeActive(edge) {
  return selectedConnection.value === connectionKey(edge.first, edge.second)
    || edge.first === selectedId.value || edge.second === selectedId.value
    || Boolean(draggingId.value && (edge.first === draggingId.value || edge.second === draggingId.value))
}
function shortName(node) { return definitionOf(node)?.name || node.componentId }
function categoryLabel(node) { return t(`models.category.${definitionOf(node)?.category || 'unknown'}`) }
function isCompatibleTarget(id) { return Boolean(linkingId.value && canConnect(linkingId.value, id)) }
function buildPayload() {
  if (!nodes.value.length) return null
  return {
    components: [...nodes.value].sort((first, second) => Number(categoryOf(second) === 'body') - Number(categoryOf(first) === 'body')).map((node) => {
      const hostBody = connections.value
        .filter((edge) => edge.first === node.id || edge.second === node.id)
        .map((edge) => nodeMap.value.get(edge.first === node.id ? edge.second : edge.first))
        .find((candidate) => definitionOf(candidate)?.category === 'body')
      return {
        id: node.id,
        component_id: node.componentId,
          placement: {
          // Internal-organ coordinates are derived by the server from the host body.
          position: categoryOf(node) === 'internal_organ'
            ? null
            : { x: Math.round((node.x - ORIGIN.x) / DISPLAY_SCALE), y: Math.round((node.y - ORIGIN.y) / DISPLAY_SCALE) },
          rotation_mdeg: node.rotationMdeg,
          size: { width_px: componentSize(node).width, length_px: componentSize(node).length },
          host_body: definitionOf(node)?.category === 'body' ? null : node.hostBody || hostBody?.id || null,
          surface_anchor: node.anchorPpm == null ? null : { perimeter_ppm: node.anchorPpm },
        },
      }
    }),
    connections: connections.value.map(({ first, second }) => ({ first, second })),
  }
}
defineExpose({ buildPayload, addComponent, reset })
</script>

<template>
  <div class="assembly-shell">
    <div class="assembly-palette">
      <span class="eyebrow">{{ t('models.available') }}</span>
      <button v-for="component in components" :key="component.id" class="assembly-part" @click="addComponent(component.id)">
        <span class="part-glyph" :class="component.category"></span>
        <span><b>{{ component.name }}</b><small>{{ t(`models.category.${component.category}`) }}</small></span>
        <span class="part-add">+</span>
      </button>
      <div v-if="!components.length" class="assembly-empty">{{ t('models.noneAvailable') }}</div>
    </div>

    <div class="assembly-workspace">
      <div class="assembly-toolbar">
        <span><MousePointer2 :size="14" /> {{ t('models.assemblyCanvas') }}</span>
        <div>
          <button class="icon-btn" :class="{ active: interactionMode === 'select' }" :title="t('models.selectMode')" @click="setInteractionMode('select')"><MousePointer2 :size="16" /></button>
          <button class="icon-btn" :class="{ active: interactionMode === 'move-component' }" :title="t('models.moveComponentMode')" @click="setInteractionMode('move-component')"><Move :size="16" /></button>
          <button class="icon-btn" :class="{ active: interactionMode === 'pan' }" :title="t('models.panCanvasMode')" @click="setInteractionMode('pan')"><Hand :size="16" /></button>
          <span class="toolbar-divider"></span>
          <button class="icon-btn" :disabled="zoom <= MIN_ZOOM" :title="t('models.zoomOut')" @click="zoomBy(-ZOOM_STEP)"><ZoomOut :size="16" /></button>
          <button class="zoom-value" :title="t('models.resetZoom')" @click="setZoom(1)">{{ zoomPercent }}</button>
          <button class="icon-btn" :disabled="zoom >= MAX_ZOOM" :title="t('models.zoomIn')" @click="zoomBy(ZOOM_STEP)"><ZoomIn :size="16" /></button>
          <button class="icon-btn" :disabled="zoom === 1" :title="t('models.resetZoom')" @click="setZoom(1)"><RotateCcw :size="15" /></button>
          <span class="toolbar-divider"></span>
          <button class="icon-btn" :class="{ active: linkingId }" :disabled="!selectedId" :title="t('models.connect')" @click="beginLink"><Link2 :size="16" /></button>
          <button class="icon-btn" :disabled="!selectedConnection" :title="t('models.removeConnection')" @click="removeConnection"><Unlink :size="16" /></button>
          <button class="icon-btn danger-icon" :disabled="!selectedNode" :title="t('models.removeComponent')" @click="removeSelected"><Trash2 :size="16" /></button>
        </div>
      </div>
      <div ref="canvas" class="assembly-canvas" :class="`mode-${interactionMode}`" @wheel.prevent="wheelZoom" @pointerdown="pointerDownCanvas" @pointermove="pointerMove" @pointerup="pointerUp" @pointercancel="pointerUp">
        <div class="assembly-scene" :style="sceneStyle">
          <div class="assembly-grid"></div>
          <svg class="assembly-links" aria-hidden="true">
            <template v-for="edge in connections" :key="connectionKey(edge.first, edge.second)">
              <line v-if="edgeLine(edge) && isEdgeActive(edge)" v-bind="edgeLine(edge)" class="link-hit" @pointerdown.stop @click.stop="selectedConnection = connectionKey(edge.first, edge.second); selectedId = ''" />
              <line v-if="edgeLine(edge)" v-bind="edgeLine(edge)" class="link-line" :class="{ active: isEdgeActive(edge), selected: selectedConnection === connectionKey(edge.first, edge.second) }" />
            </template>
          </svg>
          <button v-for="node in solidNodes" :key="node.id" class="assembly-node component-state" :class="[categoryOf(node), nodeState(node)]" :style="nodeStyle(node)" :data-node-id="node.id" :aria-label="shortName(node)" @pointerenter="hoveredId = node.id" @pointerleave="hoveredId = ''" @pointerdown.stop="pointerDown($event, node)">
            <span class="anatomy-visual">
              <span class="anatomy-shape"><span class="anatomy-detail"></span><span class="anatomy-detail-secondary"></span><span class="anatomy-detail-tertiary"></span></span>
            </span>
          </button>
          <svg class="surface-layer">
            <g v-for="node in externalNodes" :key="node.id" class="surface-organ component-state" :style="nodeStyle(node)" :class="nodeState(node)" :data-node-id="node.id" :aria-label="shortName(node)" @pointerenter="hoveredId = node.id" @pointerleave="hoveredId = ''" @pointerdown.stop="pointerDown($event, node)">
              <path class="surface-hit" :d="externalPath(node, surfaceOffset(node))" />
              <path class="surface-frame" :d="externalPath(node, surfaceOffset(node) + 5)" />
              <path class="surface-solid" :d="externalPath(node, surfaceOffset(node))" />
            </g>
          </svg>
          <svg class="component-ranges" aria-hidden="true">
            <g v-for="node in nodes" :key="node.id" class="component-state" :class="nodeState(node)">
              <circle v-if="appearanceOf(node)?.range_radius_px" class="component-range" :cx="node.x" :cy="node.y" :r="appearanceOf(node).range_radius_px * DISPLAY_SCALE" />
            </g>
          </svg>
          <div class="component-labels">
            <div v-for="node in nodes" :key="node.id" class="component-state" :class="nodeState(node)">
              <div class="component-label" :style="labelStyle(node)" :data-label-for="node.id">
                <b>{{ shortName(node) }}</b><small>{{ node.root ? t('models.rootBody') : categoryLabel(node) }}</small>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
    <aside class="assembly-inspector">
      <div class="inspector-heading"><span class="eyebrow">{{ t('models.inspector') }}</span><Box :size="16" /></div>
      <template v-if="selectedNode">
        <div class="inspector-part-name"><span class="part-glyph" :class="selectedDefinition?.category"></span><div><b>{{ selectedDefinition?.name }}</b><small>{{ categoryLabel(selectedNode) }}</small></div></div>
        <dl class="inspector-values">
          <div><dt>{{ t('models.position') }}</dt><dd>{{ Math.round((selectedNode.x - ORIGIN.x) / DISPLAY_SCALE) }}, {{ Math.round((selectedNode.y - ORIGIN.y) / DISPLAY_SCALE) }} px</dd></div>
          <div><dt>{{ t('models.size') }}</dt><dd>{{ selectedSize.length }} × {{ selectedSize.width }} px</dd></div>
          <div><dt>{{ t('models.rotation') }}</dt><dd>{{ (selectedNode.rotationMdeg / 1000).toFixed(1) }}°</dd></div>
          <div><dt>{{ t('models.host') }}</dt><dd>{{ selectedHost?.name || '—' }}</dd></div>
        </dl>
        <div class="inspector-section"><span class="eyebrow">{{ t('models.slotsLabel') }}</span><div v-if="selectedSlots.length" class="slot-list"><div v-for="([category, capacity]) in selectedSlots" :key="category"><span>{{ t(`models.category.${category}`) }}</span><b>{{ connectionsFor(selectedNode.id, category) }} / {{ capacity }}</b></div></div><span v-else class="inspector-muted">—</span></div>
      </template>
      <div v-else class="inspector-summary"><strong>{{ nodes.length }}</strong><span>{{ t('models.components') }}</span><strong>{{ connections.length }}</strong><span>{{ t('models.connections') }}</span></div>
    </aside>
  </div>
</template>
