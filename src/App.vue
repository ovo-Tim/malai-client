<template>
  <div class="q-pa-md" style="height: 100%;">
    <q-layout view="lHh lpr lFf" style="min-height: 0; height: 100%;">
      <!-- Header with actions -->
      <q-header elevated>
        <!-- We need this br for android phones. Maybe there is a better solution -->
        <br />
        <q-toolbar>
          <q-toolbar-title>Malai Client</q-toolbar-title>
          <div class="btn-group">
            <q-btn stretch flat label="Add" @click="openAddDialog" icon="add" />
            <!-- Removed Clear Selection button: background click will clear selection -->
            <q-btn stretch flat :disabled="!hasSelection" label="Delete" icon="delete" @click="deleteSelected" />
          </div>
        </q-toolbar>
        <q-toolbar inset>
          <div class="btn-group">
            <q-checkbox v-model="phone_mode" label="Phone Mode" color="green" keep-color />
            <q-checkbox v-model="dark_mode" label="Dark Mode" color="green" keep-color />
            <!-- <q-btn stretch flat icon="dark_mode"></q-btn> -->
            <q-btn stretch flat icon="input" @click="importConf" />
            <q-btn stretch flat icon="output" @click="exportConf" />
          </div>
        </q-toolbar>
      </q-header>

      <q-page-container class="flex column" style="height: 100%;">
        <!-- List of items; clicking the list's empty background will clear selection -->
        <q-list bordered padding class="server-list" @click="onListBackgroundClick" style="height: 100%;"
          :class="{ 'phone-mode-list': phone_mode }">
          <q-item v-for="item in items" :key="item.id" clickable :class="{ 'selected': item.selected }"
            @click="onItemClick($event as MouseEvent, item)" @dblclick="openEditDialog(item)"
            @pointerdown="startLongPress($event, item)" @pointerup="cancelLongPress()" @pointerleave="cancelLongPress()"
            @pointercancel="cancelLongPress()">
            <q-item-section>
              <div class="row items-center justify-between">
                <div>
                  <div class="text-weight-semibold">{{ item.name }}
                    <q-badge v-for="t in [...new Set(item.urls.map(e => e.type || 'http'))]" :key="t"
                      :color="t === 'http' ? 'blue' : t === 'tcp' ? 'orange' : t === 'udp' ? 'green' : 'purple'"
                      class="q-ml-sm">{{ t.toUpperCase() }}</q-badge>
                  </div>
                  <div class="text-caption">Ports: {{item.urls.map(e => e.port).join(', ')
                  }}</div>
                </div>
                <div class="text-caption q-ml-md">{{ item.note }}</div>
              </div>
            </q-item-section>

            <!-- Buttons container: ensure buttons are placed in a horizontal (parallel) row -->
            <q-item-section side>
              <div class="btn-group">
                <!-- Square icon buttons (no round), aligned horizontally -->
                <q-btn dense :icon="item.running ? 'stop' : 'play_arrow'"
                  :color="item.running ? 'negative' : 'secondary'" @click.stop="toggleStartStop(item)"
                  class="square-btn" :loading="item.loading" />
                <!-- Edit button removed: clicking a selected item again opens the editor -->
                <q-btn dense icon="delete" color="negative" @click.stop="deleteOne(item)" class="square-btn" />
              </div>
            </q-item-section>
          </q-item>

          <!-- Empty state -->
          <q-item v-if="items.length === 0">
            <q-item-section>
              <div class="text-center q-pa-md text-caption">No items. Click Add to create one.</div>
            </q-item-section>
          </q-item>
        </q-list>

        <!-- Dialog for Add / Edit -->
        <q-dialog v-model="add_dialog.show">
          <q-card class="card">
            <q-card-section>
              <div class="text-h6">{{ add_dialog.editing ? 'Edit item' : 'Add item' }}</div>
            </q-card-section>

            <q-card-section>
              <q-form @submit.prevent="saveDialog">
                <q-input v-model="add_dialog.model.name" label="Name" autofocus />
                <q-input v-model="add_dialog.model.note" label="Note" type="textarea" class="q-mt-sm" />

                <div class="text-subtitle2 q-mt-md q-mb-sm">URL Entries</div>
                <div v-for="(entry, i) in add_dialog.model.urls" :key="i" class="url-entry q-mb-sm q-pa-sm">
                  <div class="row q-gutter-sm items-end">
                    <q-input v-model="entry.url" label="URL (kulfi://...)" class="col" dense
                      style="min-width: 200px;" />
                    <q-input v-model.number="entry.port" label="Port" type="number" style="width: 80px;" dense />
                    <q-select v-model="entry.type" :options="connectionTypeOptions" label="Type" emit-value map-options
                      style="width: 120px;" dense />
                    <q-btn v-if="add_dialog.model.urls.length > 1" dense flat icon="close" color="negative"
                      @click="removeUrlEntry(i)" class="square-btn" />
                  </div>
                  <q-checkbox v-if="entry.type === 'http'" v-model="entry.openInBrowser"
                    label="Open in browser when service starts" dense class="q-mt-xs" />
                </div>
                <q-btn flat dense icon="add" label="Add URL" color="primary" @click="addUrlEntry" class="q-mt-xs" />
              </q-form>
            </q-card-section>

            <q-card-actions align="right">
              <q-btn flat label="Cancel" v-close-popup @click="closeDialog" />
              <q-btn color="primary" label="Save" @click="saveDialog" />
            </q-card-actions>
          </q-card>
        </q-dialog>
      </q-page-container>

    </q-layout>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch } from 'vue'
import { uid } from 'quasar'
import { useQuasar } from 'quasar'
import { load } from '@tauri-apps/plugin-store';
import { platform } from '@tauri-apps/plugin-os';
import { invoke } from '@tauri-apps/api/core';

const $q = useQuasar()

const dark_mode = ref($q.dark.isActive)
const phone_mode = ref(false)
watch(() => dark_mode.value, (v) => {
  $q.dark.set(v)
})

type ConnectionType = 'http' | 'tcp' | 'udp' | 'tcp-udp'

interface UrlEntry {
  url: string
  port: number | null
  type: ConnectionType
  openInBrowser: boolean
}

// Persisted config shape (stored / exported)
interface ItemConfig {
  id: string
  name: string
  urls: UrlEntry[]
  note: string
}

// Old format for migration
interface OldItemConfig {
  id: string
  name: string
  url: string
  port: number | null
  note: string
  openInBrowser: boolean
  type: ConnectionType
}

// Full runtime shape (adds transient state)
interface Item extends ItemConfig {
  running: boolean
  selected: boolean
  loading: boolean
}

function defaultUrlEntry(): UrlEntry {
  return { url: '', port: null, type: 'http', openInBrowser: true }
}

function migrateOldConfig(old: OldItemConfig): ItemConfig {
  return {
    id: old.id,
    name: old.name,
    urls: [{ url: old.url, port: old.port, type: old.type || 'http', openInBrowser: old.openInBrowser }],
    note: old.note,
  }
}

function isOldFormat(item: any): item is OldItemConfig {
  return typeof item === 'object' && item !== null && typeof item.url === 'string' && !Array.isArray(item.urls)
}

function toConfig(item: Item): ItemConfig {
  const { running, selected, loading, ...config } = item
  return config
}

function toItem(config: ItemConfig): Item {
  return { ...config, running: false, selected: false, loading: false }
}

function normalizeConfig(raw: any): ItemConfig {
  if (isOldFormat(raw)) return migrateOldConfig(raw)
  return raw as ItemConfig
}

// Reactive list of items (local state). Replace with API calls if needed.
const items = ref<Item[]>([
  // Example seed data
  toItem({ id: uid(), name: 'Example Server', urls: [{ url: 'kulfi://ID52', port: 8080, type: 'http', openInBrowser: true }], note: 'Demo' })
])

// Dialog state for add / edit
const add_dialog = reactive({
  show: false,
  editing: false,
  model: toItem({ id: '', name: '', urls: [defaultUrlEntry()], note: '' }) as Item
})

// Track whether user is in multi-select mode. Long-press enters this mode.
const multiSelectMode = ref(false)

// Track long-press timer so touch devices can toggle selection
let longPressTimer: ReturnType<typeof setTimeout> | null = null
const LONG_PRESS_MS = 600

// Track last long-press to avoid normal click interfering
let lastLongPressId: string | null = null
let lastLongPressTime: number | null = null
const LONG_PRESS_IGNORE_MS = 1000 // ignore normal click for 1s after long-press on same item

const connectionTypeOptions = [
  { label: 'HTTP', value: 'http' },
  { label: 'TCP', value: 'tcp' },
  { label: 'UDP', value: 'udp' },
  { label: 'TCP+UDP', value: 'tcp-udp' },
]

// Computed helpers
const hasSelection = computed(() => items.value.some(i => i.selected))

// When selection becomes empty, automatically exit multi-select mode
watch(hasSelection, (v) => {
  if (!v) multiSelectMode.value = false
})

setupParameter().then((store) => {
  watch(() => dark_mode.value, (v) => {
    store.set('dark_mode', v)
  })
  watch(() => phone_mode.value, (v) => {
    store.set('phone_mode', v)
  })
  watch(() => items.value, (v) => {
    store.set('items', v.map(toConfig))
  }, { deep: true })
})

async function setupParameter() {
  const store = await load('store.json');
  const _dark_mode = await store.get<boolean>('dark_mode');
  if (_dark_mode !== undefined) {
    dark_mode.value = _dark_mode
  } else {
    await store.set('dark_mode', dark_mode.value);
  }

  const _phone_mode = await store.get<boolean>('phone_mode');
  if (_phone_mode !== undefined) {
    phone_mode.value = _phone_mode
  } else {
    if (await platform() === 'android') {
      phone_mode.value = true
    }
    await store.set('phone_mode', phone_mode.value);
  }

  const _items = await store.get<any[]>('items');
  if (_items !== undefined) {
    items.value = _items.map(raw => toItem(normalizeConfig(raw)))
  } else {
    await store.set('items', items.value.map(toConfig));
  }

  return store
}

// URL entry management in dialog
function addUrlEntry() {
  add_dialog.model.urls.push(defaultUrlEntry())
}

function removeUrlEntry(index: number) {
  add_dialog.model.urls.splice(index, 1)
}

// Open add dialog with empty model
function openAddDialog() {
  add_dialog.editing = false
  add_dialog.model = toItem({ id: '', name: '', urls: [defaultUrlEntry()], note: '' })
  add_dialog.show = true
}

// Open edit dialog with a copy of the item
function openEditDialog(item: Item) {
  add_dialog.editing = true
  add_dialog.model = { ...item, urls: item.urls.map(e => ({ ...e })) }
  add_dialog.show = true
  clearSelection()
}

function closeDialog() {
  add_dialog.show = false
}

// Validate dialog inputs before saving
function validateDialog(): string | null {
  if (!add_dialog.model.name.trim()) {
    return 'Name is required'
  }

  if (add_dialog.model.urls.length === 0) {
    return 'At least one URL entry is required'
  }

  for (let i = 0; i < add_dialog.model.urls.length; i++) {
    const entry = add_dialog.model.urls[i]

    // Validate URL format
    if (!entry.url.trim()) {
      return `URL is required for entry ${i + 1}`
    }
    if (!entry.url.trim().startsWith('kulfi://')) {
      return `URL must start with "kulfi://" for entry ${i + 1}`
    }

    // Validate port
    if (entry.port === null || entry.port === undefined) {
      return `Port is required for entry ${i + 1}`
    }
    if (!Number.isInteger(entry.port) || entry.port < 1 || entry.port > 65535) {
      return `Port must be between 1 and 65535 for entry ${i + 1}`
    }
  }

  return null
}

// Save the dialog: create or update item
function saveDialog() {
  // Validate inputs
  const error = validateDialog()
  if (error) {
    $q.notify({
      type: 'negative',
      message: error
    })
    return
  }

  if (add_dialog.editing) {
    const idx = items.value.findIndex(i => i.id === add_dialog.model.id)
    if (idx !== -1) {
      items.value[idx] = { ...add_dialog.model, urls: add_dialog.model.urls.map(e => ({ ...e })) }
    }
  } else {
    const newItem: Item = toItem({ ...toConfig(add_dialog.model), id: uid() })
    items.value.unshift(newItem)
  }
  add_dialog.show = false
}

// Delete one item
function deleteOne(item: Item) {
  items.value = items.value.filter(i => i.id !== item.id)
}

// Delete selected items (also exit multi-select mode)
function deleteSelected() {
  items.value = items.value.filter(i => !i.selected)
  multiSelectMode.value = false
}

// Clear all selections and exit multi-select mode
function clearSelection() {
  items.value.forEach(i => { i.selected = false })
  multiSelectMode.value = false
}

function invokeForEntry(entry: UrlEntry): Promise<unknown> {
  const connType = entry.type || 'http'
  let cmd: string
  let args: Record<string, unknown>

  switch (connType) {
    case 'tcp':
      cmd = 'tcp_connect'
      args = { port: entry.port, url: entry.url }
      break
    case 'udp':
      cmd = 'udp_connect'
      args = { port: entry.port, url: entry.url }
      break
    case 'tcp-udp':
      cmd = 'tcp_udp_connect'
      args = { port: entry.port, url: entry.url }
      break
    default:
      cmd = 'browse'
      args = { port: entry.port, url: entry.url, openBrowser: entry.openInBrowser }
      break
  }

  return invoke(cmd, args)
}

// Toggle start / stop state for an item (all URL entries together)
async function toggleStartStop(item: Item) {
  const idx = items.value.findIndex(i => i.id === item.id)
  if (idx === -1) return

  items.value[idx].loading = true

  const results = await Promise.allSettled(
    items.value[idx].urls.map(entry => invokeForEntry(entry))
  )

  items.value[idx].loading = false

  // Check for rejected promises (actual errors)
  const rejectedResults = results.filter(r => r.status === 'rejected')
  if (rejectedResults.length > 0) {
    const errorMsg = (rejectedResults[0] as PromiseRejectedResult).reason
    $q.notify({
      type: 'negative',
      message: `Failed to start service: ${String(errorMsg)}`
    })
    return
  }

  const allOk = results.every(r => r.status === 'fulfilled' && r.value === 'Ok')
  const allStopped = results.every(r => r.status === 'fulfilled' && r.value === 'Stopped')

  if (allOk) {
    items.value[idx].running = true
  } else if (allStopped) {
    items.value[idx].running = false
  } else {
    // Mixed results — check if any errors
    const errors = results
      .filter(r => r.status === 'fulfilled' && r.value !== 'Ok' && r.value !== 'Stopped')
      .map(r => (r as PromiseFulfilledResult<unknown>).value)

    if (errors.length > 0) {
      $q.notify({
        type: 'negative',
        message: String(errors[0])
      })
    }

    // If some started and some stopped, try to determine the dominant state
    const okCount = results.filter(r => r.status === 'fulfilled' && r.value === 'Ok').length
    const stoppedCount = results.filter(r => r.status === 'fulfilled' && r.value === 'Stopped').length
    items.value[idx].running = okCount > stoppedCount
  }
}

// Handle click on an item: supports ctrl+click multi-select, multiSelectMode, and normal selection
function onItemClick(ev: MouseEvent, item: Item) {
  // If the user clicked on a control inside the item, don't toggle selection (buttons use .stop)
  // If the click immediately follows a long-press on the same item, ignore it
  if (lastLongPressId === item.id && lastLongPressTime && (Date.now() - lastLongPressTime) < LONG_PRESS_IGNORE_MS) {
    // reset tracking so future clicks work normally
    lastLongPressId = null
    lastLongPressTime = null
    return
  }

  // remember previous selected state to support "click again to edit" behavior
  const wasSelected = item.selected

  const isModifier = ev.ctrlKey || (ev.metaKey)
  if (isModifier) {
    // Ctrl/Cmd click toggles selection
    item.selected = !item.selected
    // entering multi-select via modifier
    if (item.selected) multiSelectMode.value = true
    return
  }

  if (multiSelectMode.value) {
    // In multi-select mode (entered by long-press or modifier), clicking toggles selection
    item.selected = !item.selected
    return
  }

  // Without modifier and not in multi-select mode:
  if (wasSelected) {
    // If this item was already selected, clicking it again opens editor
    openEditDialog(item)
    return
  }

  // otherwise select only this item
  items.value.forEach(i => { i.selected = false })
  item.selected = true
}

// Start long-press detection for touch devices. On long press, toggle selection for that item and enter multi-select mode
function startLongPress(_ev: PointerEvent, item: Item) {
  cancelLongPress()
  longPressTimer = setTimeout(() => {
    // Toggle selection and enable multi-select mode
    item.selected = !item.selected
    multiSelectMode.value = true

    // record last long-press to help ignore the immediate click that follows
    lastLongPressId = item.id
    lastLongPressTime = Date.now()
    longPressTimer = null
  }, LONG_PRESS_MS)
}

function cancelLongPress() {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
}

// If user clicks the list background (not an item), clear selection. Use target/currentTarget to detect background clicks.
function onListBackgroundClick(ev: MouseEvent) {
  if (!hasSelection.value) return
  // Only clear when the click was directly on the list element (not a child item)
  if ((ev.target as EventTarget) === (ev.currentTarget as EventTarget)) {
    clearSelection()
  }
}

function isValidUrlEntry(entry: any): boolean {
  return (
    typeof entry === 'object' && entry !== null &&
    typeof entry.url === 'string' &&
    (typeof entry.port === 'number' || entry.port === null) &&
    (entry.type === undefined || ['http', 'tcp', 'udp', 'tcp-udp'].includes(entry.type)) &&
    typeof entry.openInBrowser === 'boolean'
  )
}

function isValidItemConfigArray(data: any[]) {
  if (!Array.isArray(data)) return false;

  return data.every(item => {
    if (typeof item !== 'object' || item === null) return false
    if (typeof item.id !== 'string' || typeof item.name !== 'string' || typeof item.note !== 'string') return false

    // New format: has urls array
    if (Array.isArray(item.urls)) {
      return item.urls.every(isValidUrlEntry)
    }

    // Old format: has url string
    if (typeof item.url === 'string') {
      return (
        (typeof item.port === 'number' || item.port === null) &&
        typeof item.openInBrowser === 'boolean' &&
        (item.type === undefined || ['http', 'tcp', 'udp', 'tcp-udp'].includes(item.type))
      )
    }

    return false
  });
}

function importConf() {
  $q.dialog({
    title: 'Import configuration',
    message: 'Input the configuration to import:',
    prompt: {
      model: '',
      type: 'textarea'
    },
    cancel: true,
  }).onOk(newConf => {
    newConf = JSON.parse(newConf)
    if (isValidItemConfigArray(newConf)) {
      $q.dialog({
        title: 'Mode',
        message: 'Choose a mode:',
        options: {
          type: 'radio',
          model: 'append',
          inline: true,
          items: [
            { label: 'Append', value: 'append' },
            { label: 'Cover', value: 'cover' },
          ]
        },
        cancel: true,
      }).onOk(opt => {
        const normalized = newConf.map((raw: any) => toItem(normalizeConfig(raw)))
        if (opt === 'append') {
          items.value.push(...normalized)
        } else {
          items.value = normalized
        }
      })
    } else {
      $q.dialog({
        title: 'Alert',
        message: 'Your configuration is not valid'
      })
    }
  })
}
function exportConf() {
  const configJson = JSON.stringify(items.value.map(toConfig), null, 2)
  $q.dialog({
    title: 'Export Configuration',
    message: 'Copy the configuration below:',
    prompt: {
      model: configJson,
      type: 'textarea',
      style: 'height: 100%;',
      readonly: true
    },
    style: 'width: 95vw; max-width: 600px; min-height: 50vh;',
    ok: {
      label: 'Close',
      color: 'primary'
    }
  }).onOk(() => {
    // Copy to clipboard
    navigator.clipboard.writeText(configJson).then(() => {
      $q.notify({
        type: 'positive',
        message: 'Configuration copied to clipboard'
      })
    }).catch(() => {
      // Silently fail if clipboard not available
    })
  })
}

</script>

<style scoped>
/* stronger highlight for selected items and left accent bar for clarity */
.server-list .q-item.selected {
  background-color: rgba(0, 150, 136, 0.22);
  /* stronger highlight */
  border-left: 6px solid rgba(0, 150, 136, 0.95);
  padding-left: 10px;
  /* keep content clear of the border */
}

/* small responsive tweaks */
.server-list .q-item {
  transition: background-color 0.12s ease, border-left 0.12s ease;
}

/* square button style: fixed width/height and small corner radius */
.square-btn {
  width: 36px;
  height: 36px;
  min-width: 36px;
  border-radius: 6px;
  padding: 0;
}

/* ensure buttons are laid out horizontally and centered vertically */
.btn-group {
  display: flex;
  flex-direction: row;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  /* space between buttons */
}

/* prevent wrapping so buttons stay on one line */
.btn-group {
  flex-wrap: nowrap;
}

.phone-mode-list {
  display: flex;
  flex-direction: column-reverse;
}

/* URL entry card in the dialog */
.url-entry {
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
}

.card {
  width: 80%;
  max-width: 100vw;
}
</style>