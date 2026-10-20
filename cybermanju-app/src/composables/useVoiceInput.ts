// CyberManju OS — UseVoiceInput (src/composables/useVoiceInput.ts).
// Voice input for the agent prompt, the code editor, and cybsh.
//
// Wraps VueUse `useSpeechRecognition` with a mode-aware correction
// pipeline (src/utils/speechCorrect): final transcripts are normalised
// (spoken punctuation → marks, code words → symbols, shell homophones →
// verbs) and delivered via `dictateInto` (append to a field) or
// `dictateWith` (per-transcript callback — the editor inserts at the
// cursor). Interim results are exposed for a live "hearing…" hint.
// Unsupported browsers get `isSupported=false` so mic buttons hide
// instead of failing.
//
// Languages: `en-US` + `pt-BR`. The default comes from the browser
// (`navigator.language` → pt-* ⇒ pt-BR) and the user's pick persists in
// localStorage, so a Brazilian user dictates in Portuguese on every visit
// without re-selecting. Switching `lang` re-targets both the STT engine
// (reactive `useSpeechRecognition` lang) and the correction tables.

import { computed, ref, watch, type Ref } from 'vue'
import { useSpeechRecognition } from '@vueuse/core'
import {
  correctShellLine,
  detectVoiceLang,
  isPtLang,
  normalizeSpoken,
  resolveVoiceLang,
  VOICE_LANG_STORAGE_KEY,
  type VoiceLang,
  type VoiceLangPref,
  type VoiceMode,
} from '../utils/speechCorrect'

export type { VoiceLang, VoiceLangPref, VoiceMode }
export { VOICE_LANG_STORAGE_KEY, detectVoiceLang, isPtLang, resolveVoiceLang }

export interface VoiceInsert {
  /** Text after the mode pipeline (what was actually inserted). */
  text: string
  /** Shell-mode dictionary fixes applied, e.g. `lss→ls`. Empty otherwise. */
  fixes: string[]
}

function loadStoredLangPref(): VoiceLangPref {
  try {
    const raw = localStorage.getItem(VOICE_LANG_STORAGE_KEY)
    if (raw === 'pt-BR' || raw === 'en-US' || raw === 'auto') return raw
  } catch {
    /* private mode / SSR */
  }
  // Default: neither EN nor PT picked → autodetect from the browser locale.
  return 'auto'
}

export function useVoiceInput(mode: VoiceMode = 'prose', initialLangPref?: VoiceLangPref) {
  // Explicit pick (EN/PT) or `auto`. Auto re-resolves via detectVoiceLang()
  // on every dictation start, so it tracks the user's locale without a click.
  const langPref = ref<VoiceLangPref>(initialLangPref ?? loadStoredLangPref())
  const lang = ref<VoiceLang>(resolveVoiceLang(langPref.value))
  const rec = useSpeechRecognition({ lang, continuous: true })
  const activeMode = ref<VoiceMode>(mode)
  const lastInsert = ref<VoiceInsert | null>(null)
  const lastError = ref('')

  const listening = computed(() => rec.isListening.value)
  const interim = computed(() => (rec.isFinal.value ? '' : rec.result.value))

  watch(
    () => rec.error.value,
    (e) => {
      lastError.value = e ? String((e as Error)?.message ?? e) : ''
    },
  )

  watch(langPref, (v) => {
    try {
      localStorage.setItem(VOICE_LANG_STORAGE_KEY, v)
    } catch {
      /* private mode — lang still applies for this session */
    }
    // Keep the engine language in sync; `auto` re-detects right now.
    lang.value = resolveVoiceLang(v)
  })

  function setLang(next: VoiceLangPref): void {
    if (langPref.value === next) return
    const wasListening = rec.isListening.value
    // VueUse only applies `lang` to the engine while idle, so restart.
    if (wasListening) {
      try {
        rec.stop()
      } catch {
        /* already stopped */
      }
    }
    langPref.value = next
  }

  function toggleLang(): VoiceLang {
    const next: VoiceLang = lang.value === 'pt-BR' ? 'en-US' : 'pt-BR'
    setLang(next)
    return next
  }

  function transform(raw: string): VoiceInsert {
    // In `auto` mode the STT engine follows the browser locale, but the
    // correction tables use the pt-BR superset (EN triggers + PT twins) so
    // a Portuguese utterance through an en-US engine still gets its
    // `vírgula` → `,` / `nova linha` → `\n` fixes.
    const tableLang: VoiceLang = langPref.value === 'auto' ? 'pt-BR' : lang.value
    if (activeMode.value === 'shell') {
      const fixed = correctShellLine(normalizeSpoken(raw, 'shell', tableLang))
      return { text: fixed.line, fixes: fixed.fixes }
    }
    return { text: normalizeSpoken(raw, activeMode.value, tableLang), fixes: [] }
  }

  /**
   * Start dictation, calling `handler` with each FINAL corrected transcript.
   * The editor uses this to insert code at the cursor; `dictateInto` is the
   * append-to-field specialisation. Returns an unsubscribe fn — call it (or
   * `stop()`) on unmount or when toggling off.
   */
  function dictateWith(handler: (ins: VoiceInsert, raw: string) => void): () => void {
    let consumed = ''
    const stopWatch = watch(
      () => rec.result.value,
      (text) => {
        if (!rec.isFinal.value || !text || text === consumed) return
        consumed = text
        const ins = transform(text)
        lastInsert.value = ins
        handler(ins, text)
      },
    )
    try {
      // Auto mode: autodetect on every press, so a locale change (or a
      // fresh browser default) applies without the user picking EN/PT.
      if (langPref.value === 'auto') lang.value = detectVoiceLang()
      rec.start()
    } catch (e) {
      lastError.value = e instanceof Error ? e.message : String(e)
    }
    return () => {
      stopWatch()
      try {
        rec.stop()
      } catch {
        /* already stopped */
      }
    }
  }

  /**
   * Start dictation into `target`: each FINAL transcript is corrected and
   * appended (space-joined). Stops automatically appending on `stop()`.
   * Returns an unsubscribe fn — call it (or `stop()`) on unmount.
   */
  function dictateInto(target: Ref<string>): () => void {
    return dictateWith((ins) => {
      target.value = target.value ? `${target.value} ${ins.text}` : ins.text
    })
  }

  function toggle() {
    try {
      rec.toggle()
    } catch (e) {
      lastError.value = e instanceof Error ? e.message : String(e)
    }
  }

  return {
    isSupported: rec.isSupported,
    listening,
    interim,
    isFinal: rec.isFinal,
    confidence: rec.confidence,
    lastInsert,
    lastError,
    mode: activeMode,
    lang,
    langPref,
    setLang,
    toggleLang,
    transform,
    dictateWith,
    dictateInto,
    toggle,
    start: rec.start,
    stop: rec.stop,
  }
}
