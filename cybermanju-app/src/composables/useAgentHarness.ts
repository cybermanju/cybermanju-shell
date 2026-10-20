// CyberManju — agent harness: one friendly front door over every transport.
//
// AgentPanel (full OS window) and CodeStudio (editor sidecar) used to each
// re-implement transport branching, capability summaries and composer helpers.
// This module is the shared, UI-agnostic surface they both consume:
//
// - transport detection (`tauri` desktop · `rest` Docker/web · `wasm` Pages)
// - capability summary (model / persona / working dir / shell / key state)
// - quick prompts + slash commands for the composer
// - clipboard + token formatting helpers
// - re-exports of the loop drivers so panels import from exactly one place
//
// The permission gate, async-job contract (202 + poll), error prefixes and
// hash-anchored edits all live underneath — this harness never bypasses them,
// it only renders them in human language.

import { computed } from 'vue'
import { isStaticHost } from '@/composables/useTauri'
import {
  contextWindowFor,
  estimateTranscriptTokens,
  permissionLabel,
  toolMeta,
  type ToolPermission,
} from '@/utils/agentUi'
import type { AgentConfig, AgentSession } from '@/types'
import { copyToClipboard } from '@/utils/clipboard'

export type AgentTransport = 'tauri' | 'rest' | 'wasm'

/** Active transport. Static hosts (Pages) always run the browser loop. */
export function detectTransport(): AgentTransport {
  try {
    if (isStaticHost()) return 'wasm'
  } catch {
    /* SSR / tests — fall through to tauri default */
  }
  return 'tauri'
}

export function useAgentHarness() {
  const transport = computed<AgentTransport>(() => detectTransport())
  const isLocal = computed(() => transport.value === 'wasm')

  const transportLabel = computed(() =>
    isLocal.value ? 'On-device · browser volume + cybsh-subset' : 'Desktop · full tools',
  )

  return {
    transport,
    isLocal,
    transportLabel,
    capabilitySummary,
    quickPrompts,
    slashCommands,
    formatTokens,
    copyText,
    contextPctOf,
  }
}

// ─── capability summary ──────────────────────────────────────────────

export interface CapabilitySummary {
  model: string
  persona: 'Build' | 'Plan'
  personaHint: string
  workingDir: string
  shell: string
  permission: string
  hasKey: boolean
  keyHint: string
}

/** Human-language version of the old ALL-CAPS capability strip. */
export function capabilitySummary(
  cfg: AgentConfig | null,
  viewing: AgentSession | null,
  wasmMode: boolean,
): CapabilitySummary {
  const model = cfg?.model || viewing?.model || 'No model selected'
  const kind = cfg?.agentKind ?? viewing?.agentKind ?? 'build'
  const hasKey = !!cfg?.hasKey
  return {
    model,
    persona: kind === 'plan' ? 'Plan' : 'Build',
    personaHint:
      kind === 'plan'
        ? 'Read-only — the agent can look but never edit, write or run commands.'
        : 'Full access within your permission rules — edits and commands ask first unless allowed.',
    workingDir: cfg?.workingDir || viewing?.workingDir || '/',
    shell: (() => {
      const mode = cfg?.shellMode ?? 'auto'
      if (wasmMode) return 'Browser volume + cybsh-subset (device shell via dashboard)'
      if (mode === 'cybsh') return 'cybsh volume shell'
      if (mode === 'device') return 'Device shell'
      return 'Auto (cybsh, device fallback)'
    })(),
    permission: permissionLabel(cfg?.permission),
    hasKey,
    keyHint: hasKey
      ? 'A provider key is sealed for this config.'
      : wasmMode
        ? 'No key in memory yet — add one in Setup to start chatting.'
        : 'No key sealed yet — add one in Setup to start chatting.',
  }
}

// ─── composer aids ───────────────────────────────────────────────────

export interface QuickPrompt {
  label: string
  prompt: string
  icon: string
}

export const quickPrompts: QuickPrompt[] = [
  {
    label: 'Explain this repo',
    prompt: 'List the top-level files, then explain what this repo does in 5 bullets.',
    icon: 'solar:file-text-bold',
  },
  {
    label: 'Find bugs',
    prompt: 'Scan the working directory for likely bugs and rank the top 5 by risk, with file:line for each.',
    icon: 'solar:shield-check-bold',
  },
  {
    label: 'Plan a change',
    prompt: 'Propose a small, step-by-step plan for my next change. Do not edit anything yet — ask before each step.',
    icon: 'solar:pen-bold',
  },
  {
    label: 'Write a test',
    prompt: 'Pick the most important untested file you can see and draft a focused test for it.',
    icon: 'solar:check-circle-bold',
  },
]

export interface SlashCommand {
  cmd: string
  hint: string
  insert: string
}

export const slashCommands: SlashCommand[] = [
  { cmd: '/read', hint: 'Read a file into context', insert: '/read /path/to/file' },
  { cmd: '/edit', hint: 'Ask for a precise file edit', insert: '/edit /path/to/file — describe the change' },
  { cmd: '/plan', hint: 'Plan before acting (read-only)', insert: 'Plan only, do not edit: ' },
  { cmd: '/compact', hint: 'Summarize into a fresh session', insert: 'Summarize this thread so we can continue fresh: ' },
]

/** 12.4k / 1.0M — shared by the context meter and the thread footer. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

/** Context pressure 0–100 for any transcript + model pair. */
export function contextPctOf(messages: AgentSession['messages'], model: string): number {
  const win = contextWindowFor(model || '') || 1
  const used = estimateTranscriptTokens(messages)
  return Math.max(0, Math.min(100, Math.round((used / win) * 100)))
}

export async function copyText(text: string): Promise<boolean> {
  return copyToClipboard(text)
}

// ─── single-import surface ───────────────────────────────────────────
// Panels import the harness plus everything they render from one place;
// the underlying contracts (decide gate, 202 jobs, error prefixes,
// BLAKE3 anchors) are unchanged underneath.
export { toolMeta, permissionLabel, contextWindowFor, estimateTranscriptTokens }
export type { ToolPermission }
