<script setup lang="ts">
import { computed } from 'vue'

import { errorMessage } from '@/lib/errors'

const props = defineProps<{ error: unknown }>()
const message = computed(() => (props.error ? errorMessage(props.error) : ''))
</script>

<template>
  <div v-if="message" role="alert" class="alert alert-error">
    <span>{{ message }}</span>
  </div>
</template>
