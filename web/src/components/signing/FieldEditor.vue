<script setup lang="ts">
import { onKeyStroke } from '@vueuse/core'
import { Calendar, SquareCheck, PenLine, Signature, Trash2, Type } from 'lucide-vue-next'
import { computed, nextTick, ref, watch, type Component } from 'vue'

import Checkbox from '@/components/ui/Checkbox.vue'
import Select from '@/components/ui/Select.vue'
import { MAX_FIELDS, MAX_LABEL, type SigningFieldKind } from '@/queries/signing'

import { KIND_LABELS, localKey, mayBeOptional, type FieldDraft, type SignerDraft } from './draft'
import FieldBox from './FieldBox.vue'
import { dragDelta, moveRect, nudgeRect, placeRect, resizeRect } from './fieldEditor'
import PdfPages from './PdfPages.vue'
import { signerColor } from './signerColors'

/**
 * Places a draft's fields on its pages. Pick a kind and click the page, or
 * drag the kind onto it. Drag a field to move it and its corner handle to
 * resize it. With a field focused, the arrow keys move it (Shift for big
 * steps, Alt to resize) and Delete removes it.
 */
const props = defineProps<{
  url: string
  title?: string
  signers: SignerDraft[]
  pageCount: number
}>()
const fields = defineModel<FieldDraft[]>({ required: true })

const DRAG_TYPE = 'application/x-bizkit-field'
const KINDS: { kind: SigningFieldKind; icon: Component }[] = [
  { kind: 'signature', icon: Signature },
  { kind: 'initials', icon: PenLine },
  { kind: 'date', icon: Calendar },
  { kind: 'text', icon: Type },
  { kind: 'checkbox', icon: SquareCheck },
]

const root = ref<HTMLElement>()
const armed = ref<SigningFieldKind | null>(null)
const selectedKey = ref<string | null>(null)
const forSigner = ref(props.signers[0]?.key ?? '')
watch(
  () => props.signers.map((s) => s.key),
  (keys) => {
    if (!keys.includes(forSigner.value)) forSigner.value = keys[0] ?? ''
  },
)

const signerOptions = computed(() =>
  props.signers.map((signer, index) => ({
    value: signer.key,
    label: signer.name.trim() || `Signer ${index + 1}`,
  })),
)
const pageOptions = computed(() =>
  Array.from({ length: props.pageCount }, (_, i) => ({
    value: String(i + 1),
    label: `Page ${i + 1}`,
  })),
)
const signerIndex = (key: string) => props.signers.findIndex((s) => s.key === key)
const signerName = (key: string) => signerOptions.value[signerIndex(key)]?.label ?? 'nobody'
const colorOf = (key: string) => signerColor(signerIndex(key))

const selected = computed(() => fields.value.find((f) => f.key === selectedKey.value))
const full = computed(() => fields.value.length >= MAX_FIELDS)
const canPlace = computed(() => forSigner.value !== '' && !full.value)

function describe(field: FieldDraft): string {
  const kind = field.label ? `${KIND_LABELS[field.kind]} "${field.label}"` : KIND_LABELS[field.kind]
  return `${kind} for ${signerName(field.signerKey)}, page ${field.page}`
}

async function focusField(key: string) {
  await nextTick()
  root.value?.querySelector<HTMLElement>(`[data-field="${CSS.escape(key)}"]`)?.focus()
}

function place(kind: SigningFieldKind, page: number, cx: number, cy: number) {
  if (!canPlace.value) return
  const key = localKey('field')
  fields.value = [
    ...fields.value,
    {
      key,
      signerKey: forSigner.value,
      kind,
      page,
      ...placeRect(kind, cx, cy),
      required: true,
      label: '',
    },
  ]
  armed.value = null
  selectedKey.value = key
  void focusField(key)
}

function update(key: string, change: Partial<FieldDraft>) {
  fields.value = fields.value.map((f) => (f.key === key ? { ...f, ...change } : f))
}

function remove(key: string) {
  fields.value = fields.value.filter((f) => f.key !== key)
  if (selectedKey.value === key) selectedKey.value = null
}

/**
 * A pointer click arms the kind for a click on the page. A keyboard press
 * has no point on a page to aim at, so it places the field at once in the
 * middle of the selected field's page (or page 1), where the arrow keys
 * take it from there.
 */
function onKind(kind: SigningFieldKind, event: MouseEvent) {
  if (event.detail === 0) {
    place(kind, selected.value?.page ?? 1, 0.5, 0.5)
    return
  }
  armed.value = armed.value === kind ? null : kind
}

function onKindDrag(kind: SigningFieldKind, event: DragEvent) {
  event.dataTransfer?.setData(DRAG_TYPE, kind)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy'
}

function pointOn(event: MouseEvent): [number, number] {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  return [(event.clientX - box.left) / box.width, (event.clientY - box.top) / box.height]
}

function onPageClick(page: number, event: MouseEvent) {
  if (event.target !== event.currentTarget) return
  if (armed.value) place(armed.value, page, ...pointOn(event))
  else selectedKey.value = null
}

function onPageDragOver(event: DragEvent) {
  if (!event.dataTransfer?.types.includes(DRAG_TYPE) || !canPlace.value) return
  event.preventDefault()
  event.dataTransfer.dropEffect = 'copy'
}

function onPageDrop(page: number, event: DragEvent) {
  const kind = event.dataTransfer?.getData(DRAG_TYPE) as SigningFieldKind | undefined
  if (!kind || !KINDS.some((k) => k.kind === kind)) return
  event.preventDefault()
  place(kind, page, ...pointOn(event))
}

/** A move or resize under way: where it started, and the page's size. */
let gesture:
  | {
      key: string
      mode: 'move' | 'resize'
      from: { x: number; y: number }
      start: FieldDraft
      size: { width: number; height: number }
    }
  | undefined

function startGesture(
  field: FieldDraft,
  mode: 'move' | 'resize',
  size: { width: number; height: number },
  event: PointerEvent,
) {
  if (event.button !== 0) return
  selectedKey.value = field.key
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  gesture = {
    key: field.key,
    mode,
    from: { x: event.clientX, y: event.clientY },
    start: field,
    size,
  }
}

function onGestureMove(event: PointerEvent) {
  if (!gesture) return
  const [dx, dy] = dragDelta(gesture.from, { x: event.clientX, y: event.clientY }, gesture.size)
  const rect =
    gesture.mode === 'move' ? moveRect(gesture.start, dx, dy) : resizeRect(gesture.start, dx, dy)
  update(gesture.key, rect)
}

function endGesture() {
  gesture = undefined
}

function onFieldKey(field: FieldDraft, event: KeyboardEvent) {
  if (event.key === 'Delete' || event.key === 'Backspace') {
    event.preventDefault()
    remove(field.key)
    return
  }
  if (event.key === 'Escape') {
    selectedKey.value = null
    return
  }
  const rect = nudgeRect(field, event.key, event)
  if (!rect) return
  event.preventDefault()
  update(field.key, rect)
}

onKeyStroke('Escape', () => {
  armed.value = null
})
</script>

<template>
  <div ref="root" class="flex flex-col gap-4">
    <div
      class="sticky top-0 z-10 flex flex-col gap-3 rounded-box border border-base-300 bg-base-100 p-3 shadow-sm"
      role="toolbar"
      aria-label="Field tools"
    >
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-medium">Add a field for</span>
        <div class="w-48">
          <Select
            v-model="forSigner"
            :options="signerOptions"
            label="Signer for new fields"
            size="sm"
          />
        </div>
        <span
          v-if="forSigner"
          class="size-3 shrink-0 rounded-full"
          :class="colorOf(forSigner).swatch"
          aria-hidden="true"
        ></span>
        <div class="flex flex-wrap gap-1" role="group" aria-label="Field kinds">
          <button
            v-for="item in KINDS"
            :key="item.kind"
            type="button"
            draggable="true"
            class="btn gap-1.5 btn-sm"
            :class="armed === item.kind ? 'btn-primary' : 'btn-ghost'"
            :aria-pressed="armed === item.kind"
            :disabled="!canPlace"
            :data-kind="item.kind"
            @click="onKind(item.kind, $event)"
            @dragstart="onKindDrag(item.kind, $event)"
          >
            <component :is="item.icon" class="size-4" aria-hidden="true" />{{
              KIND_LABELS[item.kind]
            }}
          </button>
        </div>
      </div>
      <p class="text-xs text-base-content/70" role="status">
        <template v-if="full">A request takes at most {{ MAX_FIELDS }} fields.</template>
        <template v-else-if="armed"
          >Click the page where the {{ KIND_LABELS[armed].toLowerCase() }} field goes. Escape
          cancels.</template
        >
        <template v-else
          >Pick a kind and click the page, or drag it there. Drag a field to move it, its corner to
          resize it. Arrow keys move a selected field, Shift for big steps, Alt to resize.</template
        >
      </p>

      <div
        v-if="selected"
        class="flex flex-wrap items-end gap-3 border-t border-base-300 pt-3"
        data-testid="field-settings"
      >
        <div class="flex flex-col gap-1">
          <span class="text-xs font-medium">Signer</span>
          <div class="w-48">
            <Select
              :model-value="selected.signerKey"
              :options="signerOptions"
              label="Field signer"
              size="sm"
              @update:model-value="(key) => update(selected!.key, { signerKey: key })"
            />
          </div>
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-xs font-medium">Page</span>
          <div class="w-32">
            <Select
              :model-value="String(selected.page)"
              :options="pageOptions"
              label="Field page"
              size="sm"
              @update:model-value="(page) => update(selected!.key, { page: Number(page) })"
            />
          </div>
        </div>
        <template v-if="mayBeOptional(selected.kind)">
          <label class="flex flex-col gap-1">
            <span class="text-xs font-medium">Label</span>
            <input
              :value="selected.label"
              :maxlength="MAX_LABEL"
              class="input w-48 input-sm"
              placeholder="Company"
              name="field-label"
              @input="
                (event) =>
                  update(selected!.key, { label: (event.target as HTMLInputElement).value })
              "
            />
          </label>
          <div class="flex h-8 items-center">
            <Checkbox
              :model-value="selected.required"
              label="Required"
              @update:model-value="(value) => update(selected!.key, { required: value === true })"
            />
          </div>
        </template>
        <p v-else class="flex h-8 items-center text-xs text-base-content/70">
          {{ KIND_LABELS[selected.kind] }} fields are always required.
        </p>
        <button
          type="button"
          class="btn ms-auto gap-1.5 btn-ghost btn-sm"
          @click="remove(selected.key)"
        >
          <Trash2 class="size-4" aria-hidden="true" />Remove field
        </button>
      </div>
    </div>

    <PdfPages :url="url" :title="title">
      <template #page="{ page, width, height }">
        <div
          class="absolute inset-0"
          :class="armed ? 'cursor-crosshair' : ''"
          :data-page="page"
          @click="onPageClick(page, $event)"
          @dragover="onPageDragOver"
          @drop="onPageDrop(page, $event)"
        >
          <FieldBox
            v-for="field in fields.filter((f) => f.page === page)"
            :key="field.key"
            :rect="field"
          >
            <button
              type="button"
              class="flex size-full cursor-move touch-none items-center overflow-hidden rounded-sm border-2 px-1 text-left text-xs font-medium text-base-content select-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              :class="[
                colorOf(field.signerKey).box,
                selectedKey === field.key ? 'ring-2 ring-primary ring-offset-1' : '',
              ]"
              :data-field="field.key"
              :aria-label="describe(field)"
              :aria-pressed="selectedKey === field.key"
              @focus="selectedKey = field.key"
              @pointerdown="startGesture(field, 'move', { width, height }, $event)"
              @pointermove="onGestureMove"
              @pointerup="endGesture"
              @pointercancel="endGesture"
              @keydown="onFieldKey(field, $event)"
            >
              <span class="truncate">{{
                field.kind === 'checkbox' ? '' : field.label || KIND_LABELS[field.kind]
              }}</span>
            </button>
            <span
              v-if="selectedKey === field.key"
              class="absolute -right-1.5 -bottom-1.5 size-3 cursor-se-resize touch-none rounded-full border-2 border-primary bg-base-100"
              data-testid="resize-handle"
              aria-hidden="true"
              @pointerdown.stop="startGesture(field, 'resize', { width, height }, $event)"
              @pointermove="onGestureMove"
              @pointerup="endGesture"
              @pointercancel="endGesture"
            ></span>
          </FieldBox>
        </div>
      </template>
    </PdfPages>
  </div>
</template>
