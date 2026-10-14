<script setup>
defineProps({ title: String, message: String, inputLabel: String, inputValue: String, confirmText: String, cancelText: String, destructive: Boolean })
const emit = defineEmits(['confirm', 'cancel', 'update:inputValue'])
</script>
<template>
  <div class="modal-backdrop" @click.self="emit('cancel')">
    <div class="action-dialog" role="dialog" aria-modal="true" :aria-label="title">
      <div class="modal-title"><h3>{{ title }}</h3><button class="icon-btn" aria-label="Close" @click="emit('cancel')">×</button></div>
      <p v-if="message" class="dialog-message">{{ message }}</p>
      <label v-if="inputLabel" class="modal-field">{{ inputLabel }}<input :value="inputValue" autofocus @input="emit('update:inputValue', $event.target.value)" @keyup.enter="emit('confirm')" /></label>
      <div class="modal-actions"><button class="ghost-btn" @click="emit('cancel')">{{ cancelText || 'Cancel' }}</button><button class="primary-btn" :class="{ 'dialog-danger': destructive }" @click="emit('confirm')">{{ confirmText || 'OK' }}</button></div>
    </div>
  </div>
</template>
