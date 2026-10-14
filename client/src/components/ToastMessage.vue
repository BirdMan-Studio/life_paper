<script setup>
import { onBeforeUnmount, watch } from 'vue'
const props = defineProps({ message: String, tone: { type: String, default: 'info' } })
const emit = defineEmits(['close'])
let timer
watch(() => props.message, (message) => { clearTimeout(timer); if (message) timer = setTimeout(() => emit('close'), 3600) }, { immediate: true })
onBeforeUnmount(() => clearTimeout(timer))
</script>
<template>
  <Transition name="toast"><div v-if="message" class="toast-message" :class="`toast-${tone}`" role="status"><span>{{ message }}</span><button class="toast-close" aria-label="Close" @click="emit('close')">×</button></div></Transition>
</template>
