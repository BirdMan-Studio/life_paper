<script setup>
import Sidebar from './Sidebar.vue'
import TopBar from './TopBar.vue'
defineProps({ route: String, compact: Boolean, profile: Object, wallet: Object, label: String })
defineEmits(['navigate', 'toggle', 'logout'])
</script>
<template>
  <div class="app-shell" :class="{ compact }">
    <Sidebar :route="route" :compact="compact" @navigate="$emit('navigate', $event)" @toggle="$emit('toggle')" />
    <main class="main-area">
      <TopBar :label="label" :profile="profile" :wallet="wallet" @logout="$emit('logout')" />
      <div class="main-scroll"><slot /></div>
    </main>
  </div>
</template>

<style>
.app-shell { height:100dvh; min-height:0; overflow:hidden; }
.app-shell > .sidebar { height:100%; min-height:0; flex-shrink:0; }
.app-shell > .sidebar > .brand,
.app-shell > .sidebar > .nav-label,
.app-shell > .sidebar > .sidebar-footer { flex-shrink:0; }
.app-shell > .sidebar > nav { min-height:0; overflow-y:auto; }
.app-shell > .main-area { display:flex; flex-direction:column; min-height:0; overflow:hidden; }
.app-shell .topbar { flex-shrink:0; }
.main-scroll { flex:1; min-height:0; overflow:auto; overscroll-behavior:contain; scrollbar-gutter:stable; }
.main-scroll > .builder-page { height:100%; min-height:0; }
</style>
