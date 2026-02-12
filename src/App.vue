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
                  <div class="text-weight-semibold">{{ item.name }}</div>
                  <div class="text-caption">Listen port:{{ item.port
                  }}</div>
                </div>
                <div class="text-caption q-ml-md">{{ item.note }}</div>
              </div>
            </q-item-section>

            <!-- Buttons container: ensure buttons are placed in a horizontal (parallel) row -->
            <q-item-section side style="width: 180px;">
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
          <q-card>
            <q-card-section>
              <div class="text-h6">{{ add_dialog.editing ? 'Edit item' : 'Add item' }}</div>
            </q-card-section>

            <q-card-section>
              <q-form @submit.prevent="saveDialog">
                <q-input v-model="add_dialog.model.name" label="Name" autofocus />
                <q-input v-model="add_dialog.model.url" label="URL(start with kulfi://)" class="q-mt-sm" />
                <q-input v-model.number="add_dialog.model.port" label="Listen port" type="number" class="q-mt-sm" />
                <q-input v-model="add_dialog.model.note" label="Note" type="textarea" class="q-mt-sm" />
                <q-checkbox v-model="add_dialog.model.openInBrowser" label="Open in browser when service starts"
                  class="q-mt-sm" />
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

// Persisted config shape (stored / exported)
interface ItemConfig {
  id: string
  name: string
  url: string
  port: number | null
  note: string
  openInBrowser: boolean
}

// Full runtime shape (adds transient state)
interface Item extends ItemConfig {
  running: boolean
  selected: boolean
  loading: boolean
}

function toConfig(item: Item): ItemConfig {
  const { running, selected, loading, ...config } = item
  return config
}

function toItem(config: ItemConfig): Item {
  return { ...config, running: false, selected: false, loading: false }
}

// Reactive list of items (local state). Replace with API calls if needed.
const items = ref<Item[]>([
  // Example seed data
  toItem({ id: uid(), name: 'Example Server', url: 'kulfi://ID52', port: 8080, note: 'Demo', openInBrowser: true })
])

// Dialog state for add / edit
const add_dialog = reactive({
  show: false,
  editing: false,
  model: toItem({ id: '', name: '', url: '', port: null, note: '', openInBrowser: true }) as Item
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

  const _items = await store.get<ItemConfig[]>('items');
  if (_items !== undefined) {
    items.value = _items.map(toItem)
  } else {
    await store.set('items', items.value.map(toConfig));
  }

  return store
}

// Open add dialog with empty model
function openAddDialog() {
  add_dialog.editing = false
  add_dialog.model = toItem({ id: '', name: '', url: '', port: null, note: '', openInBrowser: true })
  add_dialog.show = true
}

// Open edit dialog with a copy of the item
function openEditDialog(item: Item) {
  add_dialog.editing = true
  add_dialog.model = { ...item }
  add_dialog.show = true
  clearSelection()
}

function closeDialog() {
  add_dialog.show = false
}

// Save the dialog: create or update item
function saveDialog() {
  if (add_dialog.editing) {
    const idx = items.value.findIndex(i => i.id === add_dialog.model.id)
    if (idx !== -1) {
      items.value[idx] = { ...add_dialog.model }
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

// Toggle start / stop state for an item
function toggleStartStop(item: Item) {
  const idx = items.value.findIndex(i => i.id === item.id)
  if (idx === -1) return

  items.value[idx].loading = true

  invoke('browse', { port: items.value[idx].port, url: items.value[idx].url, openBrowser: item.openInBrowser }).then((res) => {
    console.log('Browse result:', res)
    items.value[idx].loading = false
    if (res === 'Ok') {
      items.value[idx].running = true
    } else if (res === 'Stopped') {
      items.value[idx].running = false
    } else {
      $q.notify({
        type: 'Error',
        message: String(res)
      })
    }
  })

  // setTimeout(() => {
  //   // Toggle running state. Replace with real start/stop logic (IPC / HTTP / shell) as needed.
  //   items.value[idx].running = !items.value[idx].running
  //   items.value[idx].loading = false
  // }, 1000)
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

function isValidItemConfigArray(data: ItemConfig[]) {
  if (!Array.isArray(data)) return false;

  return data.every(item => {
    return (
      typeof item === 'object' &&
      item !== null &&
      typeof item.id === 'string' &&
      typeof item.name === 'string' &&
      typeof item.url === 'string' &&
      (typeof item.port === 'number' || item.port === null) &&
      typeof item.note === 'string' &&
      typeof item.openInBrowser === 'boolean'
    );
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
        if (opt === 'append') {
          items.value.push(...newConf.map(toItem))
        } else {
          items.value = newConf.map(toItem)
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
  $q.dialog({
    title: 'Export',
    message: 'Your configurations: \n' + JSON.stringify(items.value.map(toConfig))
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
</style>