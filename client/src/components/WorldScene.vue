<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue'
import * as THREE from 'three'
const host = ref(null)
let renderer, frame, resize
onMounted(() => {
  const scene = new THREE.Scene(); scene.fog = new THREE.Fog(0x0b111d, 8, 22)
  const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 100); camera.position.set(0, 7.2, 9.5); camera.lookAt(0, 0, 0)
  renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true }); renderer.setPixelRatio(Math.min(devicePixelRatio, 2)); renderer.setClearColor(0x000000, 0); host.value.appendChild(renderer.domElement)
  resize = () => { const { width, height } = host.value.getBoundingClientRect(); renderer.setSize(width, height); camera.aspect = width / height; camera.updateProjectionMatrix() }; resize(); window.addEventListener('resize', resize)
  scene.add(new THREE.AmbientLight(0x9bb9d4, 1.5)); const light = new THREE.DirectionalLight(0x67e8f9, 2.4); light.position.set(4, 7, 2); scene.add(light)
  const terrain = new THREE.Mesh(new THREE.CylinderGeometry(4.4, 4.8, .35, 48), new THREE.MeshStandardMaterial({ color: 0x142b35, roughness: .92 })); terrain.position.y = -.2; scene.add(terrain)
  const grid = new THREE.GridHelper(8.4, 18, 0x225264, 0x183442); grid.position.y = 0; grid.material.transparent = true; grid.material.opacity = .48; scene.add(grid)
  for (let i = 0; i < 18; i++) { const angle = i * 2.399; const radius = 1.0 + (i % 5) * .58; const height = .35 + (i % 4) * .24; const mesh = new THREE.Mesh(new THREE.ConeGeometry(.13 + (i % 3) * .05, height, 6), new THREE.MeshStandardMaterial({ color: i % 3 === 0 ? 0x9ae66e : 0x3db8c4, emissive: i % 3 === 0 ? 0x182b17 : 0x09252b, emissiveIntensity: .7 })); mesh.position.set(Math.cos(angle) * radius, height / 2, Math.sin(angle) * radius); scene.add(mesh) }
  const core = new THREE.Mesh(new THREE.IcosahedronGeometry(.82, 2), new THREE.MeshStandardMaterial({ color: 0x7c5cff, emissive: 0x3b237d, emissiveIntensity: 1.1, roughness: .25, metalness: .2 })); core.position.y = .85; scene.add(core)
  const ring = new THREE.Mesh(new THREE.TorusGeometry(1.25, .018, 8, 96), new THREE.MeshBasicMaterial({ color: 0x56d9e8, transparent: true, opacity: .7 })); ring.rotation.x = Math.PI / 2.5; ring.position.y = .8; scene.add(ring)
  const animate = () => { frame = requestAnimationFrame(animate); core.rotation.y += .004; core.rotation.x += .002; ring.rotation.z += .006; renderer.render(scene, camera) }; animate()
})
onBeforeUnmount(() => { cancelAnimationFrame(frame); if (resize) window.removeEventListener('resize', resize); if (renderer) renderer.dispose() })
</script>
<template><div ref="host" class="world-canvas"></div></template>
