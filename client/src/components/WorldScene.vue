<script setup>
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import * as THREE from 'three'
import { ZoomIn, ZoomOut, LocateFixed } from 'lucide-vue-next'
import { useDataStore } from '../stores/data'
import { organismRenderParts } from '../rendering/organism'
import { createOrganismMesh } from '../rendering/organismThree'
import { createTerrainMaterial } from '../rendering/terrainMaterial'
import { createTerrainSampler } from '../rendering/terrain'

const props = defineProps({ ground: Object, showOrganisms: { type: Boolean, default: true }, showMatter: { type: Boolean, default: true }, organisms: { type: Array, default: () => [] } })
const catalog = useDataStore().allComponents
const emit = defineEmits(['tile-hover'])
const host = ref(null)
const zoomPercent = ref(100)
const scaleLabel = ref(50)
const scaleWidth = ref(60)
let changeZoom, resetView, resizeObserver
let renderer, camera, frame, resize, pointerDown, pointerMove, pointerUp, pointerHover, wheel
let layerGroups, organismGroup, terrainAt
let disposeScene
let centerOnPosition
const DEFAULT_HALF_VIEW = 160
defineExpose({ focusPosition: position => centerOnPosition?.(position) })

onMounted(() => {
  const scene = new THREE.Scene()
  camera = new THREE.OrthographicCamera(-DEFAULT_HALF_VIEW, DEFAULT_HALF_VIEW, DEFAULT_HALF_VIEW, -DEFAULT_HALF_VIEW, 0.1, 100); camera.position.z = 10
  renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true }); renderer.setPixelRatio(Math.min(devicePixelRatio, 2)); renderer.setClearColor(0, 0); host.value.appendChild(renderer.domElement)
  resize = () => {
    const { width, height } = host.value.getBoundingClientRect()
    const aspect = Math.max(width, 1) / Math.max(height, 1)
    const halfWidth = DEFAULT_HALF_VIEW * Math.max(1, aspect)
    const halfHeight = DEFAULT_HALF_VIEW * Math.max(1, 1 / aspect)
    camera.left = -halfWidth; camera.right = halfWidth
    camera.top = halfHeight; camera.bottom = -halfHeight
    camera.updateProjectionMatrix(); renderer.setSize(width, height); requestRender()
  }
  window.addEventListener('resize', resize)

  layerGroups = { ground: new THREE.Group(), matter: new THREE.Group(), organism: new THREE.Group() }; scene.add(layerGroups.ground, layerGroups.matter, layerGroups.organism); organismGroup = layerGroups.organism
  const map = new THREE.Mesh(new THREE.PlaneGeometry(1, 1))
  layerGroups.ground.add(map)
  disposeScene = () => scene.traverse(node => { node.geometry?.dispose(); node.material?.dispose() })
  const applyLayerVisibility = () => {
    Object.entries(layerGroups).forEach(([name, group]) => {
      group.visible = name === 'ground' || (name === 'organism' ? props.showOrganisms : props.showMatter)
      // Ground is always the base. Apply order to nested groups as well: Three.js
      // inherits groupOrder from the nearest Group during its render traversal.
      const order = name === 'ground' ? 0 : name === 'organism' ? 20 : 10
      group.traverse(node => {
        node.renderOrder = order + (node.userData.partOrder || 0)
        if (!node.material) return
        node.material.depthTest = false
        node.material.depthWrite = false
        node.material.transparent = false
        node.material.opacity = 1
        node.material.side = THREE.DoubleSide
      })
    })
  }
  watch(() => props.ground, data => {
    map.material.dispose()
    map.material = createTerrainMaterial(data || {})
    terrainAt = createTerrainSampler(data || {})
    emit('tile-hover', null)
    requestRender()
  }, { immediate: true })

  function requestRender() {
    if (frame != null) return
    frame = requestAnimationFrame(() => {
      frame = null
      zoomPercent.value = Math.round(camera.zoom * 100)
      const unitsPerPixel = (camera.right - camera.left) / camera.zoom / Math.max(host.value.clientWidth, 1)
      const target = unitsPerPixel * 80
      const magnitude = 10 ** Math.floor(Math.log10(target))
      scaleLabel.value = [5, 2, 1].map(n => n * magnitude).find(n => n <= target) || magnitude
      scaleWidth.value = scaleLabel.value / unitsPerPixel
      map.position.set(camera.position.x, camera.position.y, 0)
      map.scale.set((camera.right - camera.left) / camera.zoom, (camera.top - camera.bottom) / camera.zoom, 1)
      renderer.render(scene, camera)
    })
  }
  resize()
  resizeObserver = new ResizeObserver(resize)
  resizeObserver.observe(host.value)

  let dragging = false; let startX = 0; let startY = 0; let startCameraX = 0; let startCameraY = 0
  pointerDown = event => { dragging = true; startX = event.clientX; startY = event.clientY; startCameraX = camera.position.x; startCameraY = camera.position.y; host.value.setPointerCapture(event.pointerId) }
  pointerHover = event => { const rect = host.value.getBoundingClientRect(); const worldX = camera.position.x + (event.clientX - rect.left - rect.width / 2) * (camera.right - camera.left) / camera.zoom / rect.width; const worldY = camera.position.y - (event.clientY - rect.top - rect.height / 2) * (camera.top - camera.bottom) / camera.zoom / rect.height; const x = Math.floor(worldX); const y = Math.floor(-worldY); const info = terrainAt?.(x, y); if (info) emit('tile-hover', { x, y, ...info }) }
  pointerMove = event => {
    if (dragging) {
      const rect = host.value.getBoundingClientRect()
      const scaleX = (camera.right - camera.left) / camera.zoom / rect.width
      const scaleY = (camera.top - camera.bottom) / camera.zoom / rect.height
      camera.position.x = startCameraX - (event.clientX - startX) * scaleX
      camera.position.y = startCameraY + (event.clientY - startY) * scaleY
      requestRender()
    }
    pointerHover(event)
  }
  pointerUp = () => { dragging = false }
  let homePosition = { x: 0, y: 0 }
  centerOnPosition = position => {
    if (!Number.isFinite(position?.x) || !Number.isFinite(position?.y)) return
    dragging = false
    homePosition = { ...position }
    camera.position.x = position.x
    camera.position.y = -position.y
    emit('tile-hover', null)
    requestRender()
  }
  changeZoom = factor => { camera.zoom = Math.min(16, Math.max(.25, camera.zoom * factor)); camera.updateProjectionMatrix(); requestRender() }
  resetView = () => { camera.zoom = 1; camera.updateProjectionMatrix(); centerOnPosition(homePosition) }
  wheel = event => { event.preventDefault(); changeZoom(event.deltaY < 0 ? 1.12 : .89) }
  host.value.addEventListener('pointerdown', pointerDown); host.value.addEventListener('pointermove', pointerMove); host.value.addEventListener('pointerup', pointerUp); host.value.addEventListener('pointercancel', pointerUp); host.value.addEventListener('wheel', wheel, { passive: false })
  watch([() => props.showOrganisms, () => props.showMatter], () => { applyLayerVisibility(); requestRender() }, { immediate: true })
  watch([() => props.organisms, catalog], ([list]) => {
    organismGroup.traverse(node => { node.geometry?.dispose(); node.material?.dispose() }); organismGroup.clear()
    const definitions = new Map(catalog.value.map(definition => [definition.id, definition]))
    for (const item of list) {
      const creature = createOrganismMesh(organismRenderParts(item.composition || item.model?.composition || {}, definitions))
      creature.position.set(item.position?.x ?? item.x ?? 0, -(item.position?.y ?? item.y ?? 0), 0)
      creature.rotation.z = -(item.rotation_mdeg || 0) * Math.PI / 180000
      organismGroup.add(creature)
    }
    applyLayerVisibility()
    requestRender()
  }, { deep: true, immediate: true })
})

onBeforeUnmount(() => { cancelAnimationFrame(frame); resizeObserver?.disconnect(); disposeScene?.(); if (resize) window.removeEventListener('resize', resize); if (host.value) { host.value.removeEventListener('pointerdown', pointerDown); host.value.removeEventListener('pointermove', pointerMove); host.value.removeEventListener('pointerup', pointerUp); host.value.removeEventListener('pointercancel', pointerUp); host.value.removeEventListener('wheel', wheel) }; if (renderer) renderer.dispose() })
</script>

<template>
  <div ref="host" class="world-canvas">
    <div class="map-navigation" @pointerdown.stop @pointermove.stop @wheel.stop.prevent>
      <button type="button" title="放大" aria-label="放大" :disabled="zoomPercent >= 1600" @click="changeZoom(1.4)"><ZoomIn :size="18" /></button>
      <span class="map-zoom">{{ zoomPercent }}%</span>
      <button type="button" title="缩小" aria-label="缩小" :disabled="zoomPercent <= 25" @click="changeZoom(1 / 1.4)"><ZoomOut :size="18" /></button>
      <button type="button" title="回到初始视野" aria-label="回到初始视野" @click="resetView()"><LocateFixed :size="18" /></button>
    </div>
    <div class="map-scale" @pointerdown.stop :style="{ width: `${scaleWidth}px` }"><span>{{ scaleLabel }} px</span></div>
  </div>
</template>

<style scoped>
.world-canvas { position: relative; overflow: hidden; }
.world-canvas :deep(canvas) { display: block; }
.map-navigation { position: absolute; top: 12px; right: 12px; z-index: 2; display: flex; align-items: center; padding: 3px; gap: 2px; background: #fffffff0; border: 1px solid #cbd3cb; border-radius: 6px; box-shadow: 0 2px 8px #152b1920; cursor: default; }
.map-navigation button { display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: #344939; cursor: pointer; }
.map-navigation button:hover { background: #e3ebe4; }
.map-navigation button:disabled { opacity: .35; cursor: default; }
.map-navigation button:focus-visible { outline: 2px solid #3c7364; outline-offset: -2px; }
.map-zoom { width: 44px; text-align: center; font: 11px monospace; color: #435449; }
.map-scale { position: absolute; right: 16px; bottom: 16px; z-index: 2; height: 7px; border: solid #fff; border-width: 0 1px 2px; filter: drop-shadow(0 1px 1px #233629); pointer-events: none; }
.map-scale span { position: absolute; bottom: 6px; right: 0; white-space: nowrap; color: white; font: 11px monospace; }
</style>
