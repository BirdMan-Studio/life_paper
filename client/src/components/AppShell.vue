<script setup>
import Sidebar from './Sidebar.vue'
import TopBar from './TopBar.vue'
defineProps({ route: String, compact: Boolean, profile: Object, wallet: Object, label: String })
defineEmits(['navigate', 'toggle', 'logout'])
</script>
<template><div class="app-shell" :class="{ compact }"><Sidebar :route="route" :compact="compact" @navigate="$emit('navigate', $event)" @toggle="$emit('toggle')" /><main class="main-area"><TopBar :label="label" :profile="profile" :wallet="wallet" @logout="$emit('logout')" /><slot /></main></div></template>
