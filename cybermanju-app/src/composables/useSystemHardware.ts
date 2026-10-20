// CyberManju OS — System hardware & Browser API layer
// Centralises every Web capability a "web OS inside an OS" needs.
//
// Strategy: VueUse covers the standardised part (reactive, permission-aware,
// auto-updating); raw navigator APIs cover the still-unstandardised
// device-access part (USB / HID / Serial / MIDI / NFC / WebAuthn / screens).
// Panels import ONE hook — useSystemHardware() — so the whole OS shares the
// same subscriptions instead of each window re-enumerating hardware.
//
// | Capability                  | Source                                    |
// |-----------------------------|-------------------------------------------|
// | cameras/mics/speakers       | VueUse useDevicesList + enumerate         |
// | gamepads                    | VueUse useGamepad (merged into list)     |
// | battery, network/online     | VueUse useBattery/useNetwork/useOnline    |
// | geolocation                 | VueUse useGeolocation                     |
// | clipboard (text + rich)     | VueUse useClipboard/useClipboardItems     |
// | web share                   | VueUse useShare                           |
// | fullscreen                  | VueUse useFullscreen                      |
// | wake lock                   | VueUse useWakeLock                        |
// | screen orientation/safe-area| VueUse useScreenOrientation/SafeArea      |
// | eye dropper                 | VueUse useEyeDropper                      |
// | vibration                   | VueUse useVibrate                         |
// | speech synth/recognition    | VueUse useSpeechSynthesis/Recognition     |
// | web notification            | VueUse useWebNotification                 |
// | bluetooth LE                | VueUse useBluetooth                       |
// | camera/mic/screen capture   | VueUse useUserMedia/useDisplayMedia       |
// | local file open/save        | VueUse useFileSystemAccess/useFileDialog  |
// | multi-tab bus               | VueUse useBroadcastChannel                |
// | idle / visibility / focus   | VueUse useIdle/DocumentVisibility/WindowFocus |
// | memory / motion / orient.   | VueUse useMemory/DeviceMotion/Orientation |
// | locale / window size        | VueUse useNavigatorLanguage/WindowSize    |
// | theme preference            | VueUse usePreferredDark/ColorScheme       |
// | USB / HID / Serial / MIDI   | navigator.* (native, permission-gated)    |
// | NFC / contacts / barcode    | NDEFReader / navigator.contacts / BarcodeDetector |
// | face detection (local)      | FaceDetector (native, progressive)        |
// | passkeys                    | WebAuthn navigator.credentials (native)   |
// | multi-screen                | window.getScreenDetails (native)          |
// | app badge / persist quota   | navigator.setAppBadge / storage (native)  |

import { ref, computed, watch } from 'vue'
import { detectVoiceLang, VOICE_LANG_STORAGE_KEY, type VoiceLang } from '@/utils/speechCorrect'
import {
  useDevicesList,
  useBattery,
  useNetwork,
  useOnline,
  useGeolocation,
  useClipboard,
  useClipboardItems,
  useShare,
  useFullscreen,
  useWakeLock,
  useScreenOrientation,
  useScreenSafeArea,
  useEyeDropper,
  useVibrate,
  useWebNotification,
  useBluetooth,
  usePermission,
  useUserMedia,
  useDisplayMedia,
  useGamepad,
  useFileSystemAccess,
  useFileDialog,
  useBroadcastChannel,
  useIdle,
  useDocumentVisibility,
  useWindowFocus,
  useMemory,
  useDeviceMotion,
  useDeviceOrientation,
  useNavigatorLanguage,
  usePreferredLanguages,
  usePreferredDark,
  usePreferredColorScheme,
  useWindowSize,
  useSpeechRecognition,
  useSpeechSynthesis,
  useEventListener,
} from '@vueuse/core'

// ── Types ────────────────────────────────────────────────────────────────

export interface PluggedDevice {
  id: string
  bus: 'media' | 'usb' | 'bluetooth' | 'hid' | 'serial' | 'midi' | 'gamepad'
  kind: string
  label: string
  connected: boolean
  raw?: unknown
}

export interface ScreenInfo {
  label: string
  width: number
  height: number
  availWidth: number
  availHeight: number
  primary: boolean
}

interface NavigatorDeviceAPIs {
  usb?: {
    getDevices(): Promise<unknown[]>
    requestDevice(opts: unknown): Promise<unknown>
    addEventListener?: (t: string, l: EventListener) => void
    removeEventListener?: (t: string, l: EventListener) => void
  }
  hid?: {
    getDevices(): Promise<unknown[]>
    requestDevice(opts: unknown): Promise<unknown>
    addEventListener?: (t: string, l: EventListener) => void
    removeEventListener?: (t: string, l: EventListener) => void
  }
  serial?: {
    getPorts(): Promise<unknown[]>
    requestPort(opts?: unknown): Promise<unknown>
    addEventListener?: (t: string, l: EventListener) => void
    removeEventListener?: (t: string, l: EventListener) => void
  }
}

function nav(): NavigatorDeviceAPIs {
  return navigator as unknown as NavigatorDeviceAPIs
}

function labelOf(d: unknown, fallback: string): string {
  const o = d as Record<string, unknown>
  const label = o.label ?? o.name ?? o.productName
  if (typeof label === 'string' && label) return label
  return fallback
}

// ── Singleton state (one enumeration for the whole OS) ──────────────────

const usbDevices = ref<unknown[]>([])
const hidDevices = ref<unknown[]>([])
const serialPorts = ref<unknown[]>([])
const midiInputs = ref<string[]>([])
const midiOutputs = ref<string[]>([])
const refreshing = ref(false)
const lastError = ref('')
const screens = ref<ScreenInfo[]>([])
const nfcMessage = ref('')
let listenersAttached = false

function loadStoredSpeechLang(): VoiceLang | null {
  try {
    if (typeof localStorage === 'undefined') return null
    const raw = localStorage.getItem(VOICE_LANG_STORAGE_KEY)
    if (raw === 'pt-BR' || raw === 'en-US') return raw
  } catch {
    /* private mode / SSR */
  }
  return null
}

// Shared STT/TTS language (en-US + pt-BR): browser default (pt-* ⇒ pt-BR),
// user pick persists so Devices voice + Speak follow the mic selectors.
const speechLang = ref<VoiceLang>(loadStoredSpeechLang() ?? detectVoiceLang())
if (typeof window !== 'undefined') {
  watch(speechLang, (v) => {
    try {
      localStorage.setItem(VOICE_LANG_STORAGE_KEY, v)
    } catch {
      /* private mode — still applies for this session */
    }
  })
}

/** Unified "everything plugged in" list for the Devices panel / StatusBar. */
export function useSystemHardware() {
  // VueUse — reactive, permission-aware, auto-updating where the spec allows.
  // Media capture hooks stay DISABLED until the user presses a button in the
  // Devices panel (no permission prompt on OS boot).
  const { videoInputs, audioInputs, audioOutputs } = useDevicesList({ requestPermissions: false })
  const battery = useBattery()
  const network = useNetwork()
  const online = useOnline()
  const geo = useGeolocation({ enableHighAccuracy: false, immediate: false })
  const { copy, copied, isSupported: clipboardSupported } = useClipboard()
  const clipboardItems = useClipboardItems({ read: false })
  const { share, isSupported: shareSupported } = useShare()
  const fullscreen = useFullscreen()
  const wakeLock = useWakeLock()
  const orientation = useScreenOrientation()
  const safeArea = useScreenSafeArea()
  const eyeDropper = useEyeDropper()
  const { vibrate, isSupported: vibrateSupported } = useVibrate()
  const webNotification = useWebNotification()
  const bluetooth = useBluetooth({ acceptAllDevices: true })
  const cameraPermission = usePermission('camera')
  const micPermission = usePermission('microphone')
  const geoPermission = usePermission('geolocation')
  const notifyPermission = usePermission('notifications')

  // Media capture (face-grouping camera input, screen-share for demos).
  // NOTE: useUserMedia with no constraints calls
  // getUserMedia({ video: undefined, audio: undefined }) which the spec
  // rejects with "At least one of audio and video must be requested".
  // Always default to video so Start camera works out of the box.
  const userMedia = useUserMedia({ enabled: false, constraints: { video: true, audio: false } })
  const displayMedia = useDisplayMedia({ enabled: false, video: true })
  const speechRecognition = useSpeechRecognition({ lang: speechLang })
  const speechText = ref('CyberManju OS ready')
  const speechSynthesis = useSpeechSynthesis(speechText, { lang: speechLang })

  function setSpeechLang(next: VoiceLang): void {
    if (speechLang.value === next) return
    const wasListening = speechRecognition.isListening.value
    if (wasListening) {
      try {
        speechRecognition.stop()
      } catch {
        /* already stopped */
      }
    }
    speechLang.value = next
  }

  function toggleSpeechLang(): VoiceLang {
    const next: VoiceLang = speechLang.value === 'pt-BR' ? 'en-US' : 'pt-BR'
    setSpeechLang(next)
    return next
  }

  // Local files without Tauri: File System Access API + classic picker.
  const fsAccess = useFileSystemAccess({ dataType: 'Text' })
  const fileDialog = useFileDialog({ accept: '*/*', multiple: true })

  // Multi-tab + presence: one named bus, idle hint at 5 min.
  const bus = useBroadcastChannel<string, string>({ name: 'cybermanju-os' })
  const idle = useIdle(5 * 60 * 1000)
  const visibility = useDocumentVisibility()
  const focused = useWindowFocus()

  // Sensors / system telemetry for the Devices + Process panels.
  const memory = useMemory()
  const motion = useDeviceMotion()
  const deviceOrientation = useDeviceOrientation()
  const navigatorLanguage = useNavigatorLanguage()
  const preferredLanguages = usePreferredLanguages()
  const preferredDark = usePreferredDark()
  const preferredScheme = usePreferredColorScheme()
  const windowSize = useWindowSize()
  const gamepad = useGamepad()

  // Live re-enumeration: plugs/unplugs refresh the unified list.
  // Attached once per page load (module guard) — every hook call reuses it.
  if (!listenersAttached && typeof window !== 'undefined') {
    listenersAttached = true
    try {
      const md = navigator.mediaDevices
      if (md) useEventListener(md as unknown as EventTarget, 'devicechange', () => void refreshDevices())
      const n = nav()
      const re = () => void refreshDevices()
      n.usb?.addEventListener?.('connect', re)
      n.usb?.addEventListener?.('disconnect', re)
      n.hid?.addEventListener?.('connect', re)
      n.hid?.addEventListener?.('disconnect', re)
      n.serial?.addEventListener?.('connect', re)
      n.serial?.addEventListener?.('disconnect', re)
      useEventListener(window, 'gamepadconnected', re)
      useEventListener(window, 'gamepaddisconnected', re)
    } catch { /* listeners are best-effort */ }
  }

  const w = window as unknown as {
    BarcodeDetector?: new () => { detect(s: unknown): Promise<unknown[]> }
    FaceDetector?: new () => { detect(s: unknown): Promise<unknown[]> }
    getScreenDetails?: () => Promise<{
      screens: Array<{ label: string; width: number; height: number; availWidth: number; availHeight: number; primary: boolean }>
    }>
    PressureObserver?: unknown
  }
  const nExt = navigator as unknown as {
    contacts?: { select(props: string[], opts: { multiple: boolean }): Promise<unknown[]> }
    setAppBadge?: (n?: number) => Promise<void>
    clearAppBadge?: () => Promise<void>
  }

  const support = computed(() => ({
    usb: typeof nav().usb !== 'undefined',
    hid: typeof nav().hid !== 'undefined',
    serial: typeof nav().serial !== 'undefined',
    midi: typeof (navigator as unknown as { requestMIDIAccess?: unknown }).requestMIDIAccess !== 'undefined',
    bluetooth: bluetooth.isSupported.value,
    mediaDevices: typeof navigator.mediaDevices?.enumerateDevices === 'function',
    camera: userMedia.isSupported.value,
    screenShare: displayMedia.isSupported.value,
    clipboard: clipboardSupported.value,
    clipboardItems: clipboardItems.isSupported.value,
    share: shareSupported.value,
    fullscreen: fullscreen.isSupported.value,
    wakeLock: wakeLock.isSupported.value,
    vibrate: vibrateSupported.value,
    eyeDropper: eyeDropper.isSupported.value,
    geolocation: 'geolocation' in navigator,
    notifications: webNotification.isSupported.value,
    storageEstimate: typeof navigator.storage?.estimate === 'function',
    fileSystemAccess: fsAccess.isSupported.value,
    gamepad: gamepad.isSupported.value,
    speechRecognition: speechRecognition.isSupported.value,
    speechSynthesis: speechSynthesis.isSupported.value,
    broadcast: bus.isSupported.value,
    idle: true,
    memory: memory.isSupported.value,
    motion: motion.isSupported.value,
    deviceOrientation: deviceOrientation.isSupported.value,
    nfc: typeof (window as unknown as { NDEFReader?: unknown }).NDEFReader !== 'undefined',
    contacts: typeof nExt.contacts?.select === 'function',
    barcode: typeof w.BarcodeDetector !== 'undefined',
    faceDetector: typeof w.FaceDetector !== 'undefined',
    webAuthn: typeof window.PublicKeyCredential !== 'undefined',
    multiScreen: typeof w.getScreenDetails === 'function',
    appBadge: typeof nExt.setAppBadge === 'function',
    pressure: typeof w.PressureObserver !== 'undefined',
  }))

  const pluggedDevices = computed<PluggedDevice[]>(() => {
    const out: PluggedDevice[] = []
    for (const d of videoInputs.value)
      out.push({ id: d.deviceId, bus: 'media', kind: 'camera', label: d.label || `Camera (${d.deviceId.slice(0, 6)})`, connected: true, raw: d })
    for (const d of audioInputs.value)
      out.push({ id: d.deviceId, bus: 'media', kind: 'microphone', label: d.label || `Microphone (${d.deviceId.slice(0, 6)})`, connected: true, raw: d })
    for (const d of audioOutputs.value)
      out.push({ id: d.deviceId, bus: 'media', kind: 'speaker', label: d.label || `Speaker (${d.deviceId.slice(0, 6)})`, connected: true, raw: d })
    usbDevices.value.forEach((d, i) =>
      out.push({ id: `usb-${i}`, bus: 'usb', kind: 'usb', label: labelOf(d, `USB device ${i + 1}`), connected: true, raw: d }))
    hidDevices.value.forEach((d, i) =>
      out.push({ id: `hid-${i}`, bus: 'hid', kind: 'hid', label: labelOf(d, `HID device ${i + 1}`), connected: true, raw: d }))
    serialPorts.value.forEach((d, i) =>
      out.push({ id: `serial-${i}`, bus: 'serial', kind: 'serial', label: labelOf(d, `Serial port ${i + 1}`), connected: true, raw: d }))
    midiInputs.value.forEach((n, i) =>
      out.push({ id: `midi-in-${i}`, bus: 'midi', kind: 'midi-input', label: n || `MIDI input ${i + 1}`, connected: true }))
    midiOutputs.value.forEach((n, i) =>
      out.push({ id: `midi-out-${i}`, bus: 'midi', kind: 'midi-output', label: n || `MIDI output ${i + 1}`, connected: true }))
    try {
      for (const g of gamepad.gamepads.value) {
        if (!g) continue
        out.push({ id: g.id, bus: 'gamepad', kind: 'gamepad', label: `${g.id.slice(0, 40)} (${g.axes.length} axes)`, connected: g.connected, raw: g })
      }
    } catch { /* gamepad list is best-effort */ }
    if (bluetooth.device.value)
      out.push({ id: 'ble-current', bus: 'bluetooth', kind: 'bluetooth', label: labelOf(bluetooth.device.value, 'Bluetooth device'), connected: bluetooth.isConnected.value, raw: bluetooth.device.value })
    return out
  })

  const deviceCountByBus = computed(() => {
    const counts: Record<string, number> = {}
    for (const d of pluggedDevices.value) counts[d.bus] = (counts[d.bus] ?? 0) + 1
    return counts
  })

  /** Re-read every natively-enumerated bus (media list is live via VueUse). */
  async function refreshDevices(): Promise<void> {
    refreshing.value = true
    lastError.value = ''
    try {
      const n = nav()
      if (n.usb) {
        try { usbDevices.value = await n.usb.getDevices() } catch { /* permission-gated */ }
      }
      if (n.hid) {
        try { hidDevices.value = await n.hid.getDevices() } catch { /* permission-gated */ }
      }
      if (n.serial) {
        try { serialPorts.value = await n.serial.getPorts() } catch { /* permission-gated */ }
      }
      const withMidi = navigator as unknown as { requestMIDIAccess?: () => Promise<{ inputs: Map<string, { name?: string }>; outputs: Map<string, { name?: string }> }> }
      if (typeof withMidi.requestMIDIAccess === 'function') {
        try {
          const access = await withMidi.requestMIDIAccess()
          midiInputs.value = [...access.inputs.values()].map(p => p.name ?? '')
          midiOutputs.value = [...access.outputs.values()].map(p => p.name ?? '')
        } catch { /* permission-gated */ }
      }
      await refreshScreens().catch(() => {})
    } catch (e) {
      lastError.value = e instanceof Error ? e.message : String(e)
    } finally {
      refreshing.value = false
    }
  }

  // Pairing actions — each MUST be called from a user gesture (button click).
  async function requestUsb(): Promise<void> {
    try { await nav().usb?.requestDevice({ filters: [] }); await refreshDevices() }
    catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  async function requestHid(): Promise<void> {
    try { await nav().hid?.requestDevice({ filters: [] }); await refreshDevices() }
    catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  async function requestSerial(): Promise<void> {
    try { await nav().serial?.requestPort(); await refreshDevices() }
    catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  async function requestBluetooth(): Promise<void> {
    try { await bluetooth.requestDevice() }
    catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }

  // ── Media capture ──────────────────────────────────────────────
  async function startCamera(constraints: MediaStreamConstraints = { video: true, audio: false }): Promise<void> {
    try {
      // getUserMedia throws "At least one of audio and video must be
      // requested" for {} / { video: false, audio: false } / all-undefined.
      // Normalise so the camera button can never trigger that rejection.
      const safe: MediaStreamConstraints = { ...constraints }
      if (!safe.video && !safe.audio) safe.video = true
      userMedia.constraints.value = safe
      userMedia.enabled.value = true
      await userMedia.start()
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  function stopCamera(): void {
    try { userMedia.stop(); userMedia.enabled.value = false } catch { /* noop */ }
  }
  async function startScreenShare(): Promise<void> {
    try { displayMedia.enabled.value = true; await displayMedia.start() }
    catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  function stopScreenShare(): void {
    try { displayMedia.stop(); displayMedia.enabled.value = false } catch { /* noop */ }
  }
  function speak(text: string): void {
    try {
      speechText.value = text
      speechSynthesis.speak()
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }

  // ── Local files (File System Access) ──────────────────────────
  async function openLocalFile(): Promise<string | undefined> {
    try {
      await fsAccess.open()
      const d = fsAccess.data.value
      return typeof d === 'string' ? d : undefined
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e); return undefined }
  }
  async function saveLocalFile(content: string, suggestedName = 'cybermanju.txt'): Promise<void> {
    try {
      fsAccess.data.value = content
      await fsAccess.saveAs({ suggestedName })
      await fsAccess.updateData()
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  function pickFiles(): void {
    try { fileDialog.open() } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  async function copyRichHtml(html: string, textFallback: string): Promise<void> {
    try {
      if (clipboardItems.isSupported.value) {
        await clipboardItems.copy([new ClipboardItem({
          'text/html': new Blob([html], { type: 'text/html' }),
          'text/plain': new Blob([textFallback], { type: 'text/plain' }),
        })])
      } else {
        await copy(textFallback)
      }
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }

  // ── Presence / multi-tab ──────────────────────────────────────
  function pingTabs(message = 'ping'): void {
    try { bus.post(message) } catch { /* closed */ }
  }

  // ── Native progressive APIs (no VueUse equivalent) ────────────
  async function refreshScreens(): Promise<void> {
    try {
      const gsd = (window as unknown as { getScreenDetails?: () => Promise<{ screens: ScreenInfo[] }> }).getScreenDetails
      if (typeof gsd !== 'function') { screens.value = []; return }
      const details = await gsd()
      screens.value = details.screens.map(s => ({ ...s }))
    } catch { /* permission-gated */ }
  }
  async function setBadge(count?: number): Promise<void> {
    try { await nExt.setAppBadge?.(count) } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  async function clearBadge(): Promise<void> {
    try { await nExt.clearAppBadge?.() } catch { /* noop */ }
  }
  async function pickContacts(): Promise<unknown[]> {
    try {
      if (typeof nExt.contacts?.select !== 'function') { lastError.value = 'Contacts API not supported here'; return [] }
      return await nExt.contacts.select(['name', 'email', 'tel'], { multiple: true })
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e); return [] }
  }
  async function scanBarcode(source: unknown): Promise<unknown[]> {
    try {
      const BD = w.BarcodeDetector
      if (!BD) { lastError.value = 'BarcodeDetector not supported here'; return [] }
      return await new BD().detect(source)
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e); return [] }
  }
  async function detectFaces(source: unknown): Promise<unknown[]> {
    try {
      const FD = w.FaceDetector
      if (!FD) { lastError.value = 'FaceDetector not supported here'; return [] }
      return await new FD().detect(source)
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e); return [] }
  }
  async function requestNfcScan(): Promise<void> {
    try {
      const Reader = (window as unknown as { NDEFReader?: new () => { scan(): Promise<void>; addEventListener(t: string, l: (e: unknown) => void): void } }).NDEFReader
      if (!Reader) { lastError.value = 'Web NFC not supported here (needs Chrome + Android + HTTPS)'; return }
      const reader = new Reader()
      reader.addEventListener('reading', (e) => { nfcMessage.value = String((e as { message?: unknown })?.message ?? 'NFC tag read') })
      await reader.scan()
      nfcMessage.value = 'NFC scan started — tap a tag…'
    } catch (e) { lastError.value = e instanceof Error ? e.message : String(e) }
  }
  function webAuthnSupported(): boolean {
    return typeof window.PublicKeyCredential !== 'undefined'
  }

  return {
    // lists
    pluggedDevices, deviceCountByBus, refreshing, lastError, refreshDevices,
    requestUsb, requestHid, requestSerial, requestBluetooth,
    usbDevices, hidDevices, serialPorts, midiInputs, midiOutputs,
    // vueuse primitives (re-exported so panels share one subscription)
    videoInputs, audioInputs, audioOutputs,
    battery, network, online, geo,
    copy, copied, clipboardSupported, clipboardItems, share, shareSupported,
    fullscreen, wakeLock, orientation, safeArea, eyeDropper,
    vibrate, vibrateSupported, webNotification,
    bluetooth, cameraPermission, micPermission, geoPermission, notifyPermission,
    support,
    // media capture
    userMedia, displayMedia, speechRecognition, speechSynthesis,
    speechLang, setSpeechLang, toggleSpeechLang,
    startCamera, stopCamera, startScreenShare, stopScreenShare, speak,
    // local files
    fsAccess, fileDialog, openLocalFile, saveLocalFile, pickFiles, copyRichHtml,
    // presence / multi-tab
    bus, pingTabs, idle, visibility, focused,
    // sensors / system
    memory, motion, deviceOrientation, navigatorLanguage, preferredLanguages,
    preferredDark, preferredScheme, windowSize, gamepad,
    // native progressive
    screens, refreshScreens, nfcMessage, requestNfcScan,
    pickContacts, scanBarcode, detectFaces, setBadge, clearBadge, webAuthnSupported,
  }
}

// ── Storage quota (StorageManager API — no VueUse equivalent) ───────────

export interface StorageEstimate {
  quota?: number
  usage?: number
  percent?: number
  persisted: boolean
}

export async function getStorageEstimate(): Promise<StorageEstimate> {
  let persisted = false
  try { persisted = await navigator.storage?.persisted() ?? false } catch { /* noop */ }
  try {
    const est = await navigator.storage?.estimate()
    if (!est) return { persisted }
    const percent = est.quota ? Math.round(((est.usage ?? 0) / est.quota) * 100) : undefined
    return { quota: est.quota, usage: est.usage, percent, persisted }
  } catch {
    return { persisted }
  }
}

export async function persistStorage(): Promise<boolean> {
  try { return await navigator.storage?.persist() ?? false } catch { return false }
}

/** Blob/File → object URL for the preview panel (caller revokes). */
export function previewUrlOf(data: Blob | MediaSource): string {
  return URL.createObjectURL(data)
}
