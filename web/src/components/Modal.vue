<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { X } from 'lucide-vue-next'
defineProps<{ title: string; busy?: boolean }>()
const emit = defineEmits<{ close: [] }>()
const box = ref<HTMLDialogElement>()
onMounted(() => box.value?.showModal())
</script>
<template>
  <dialog ref="box" aria-labelledby="dialog-title" @cancel.prevent="!busy && emit('close')">
    <header>
      <h2 id="dialog-title">{{ title }}</h2>
      <button class="icon" :disabled="busy" title="关闭" aria-label="关闭" @click="emit('close')">
        <X />
      </button>
    </header>
    <slot />
  </dialog>
</template>
<style scoped>
dialog {
  width: min(500px, calc(100% - 32px));
  max-height: 85vh;
  overflow: auto;
  border: 1px solid var(--md-outline-variant);
  border-radius: var(--shape-xl);
  padding: var(--space-6);
  background: var(--md-surface-container-low);
  color: var(--md-on-surface);
}
dialog::backdrop {
  background: color-mix(in srgb, var(--md-inverse-surface) 42%, transparent);
}
header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-4);
}
</style>
