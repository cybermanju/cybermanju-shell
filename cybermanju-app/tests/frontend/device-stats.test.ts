// CyberManju OS — Device stats.test (tests/frontend/device-stats.test.ts).
// Regression cover for the wasm Devices-window crash ("Cannot read
// properties of undefined (reading 'toFixed')"): the wasm `top` command
// answers tasks/counts/uptimeMs ONLY, so every formatter must stay total on
// partial payloads — real readings where present, 'n/a' where the sandbox
// has none, never a throw.
import { describe, expect, it } from 'vitest'
import {
  formatBytesDec,
  formatCoord,
  formatCpuPercent,
  formatHeapLine,
  formatLoadTriplet,
  formatTopLine,
  isFiniteNum,
} from '../../src/utils/deviceStats'

/** Byte-shape the wasm `top` dispatcher answers (crates/os-wasm/src/os.rs). */
const WASM_TOP = { tasks: [], counts: { pending: 0, running: 0, done: 0, failed: 0, killed: 0, total: 0 }, pid: 1, uptimeMs: 0 }

const FULL_TOP = {
  cpuPercent: 12.34,
  uptimeMs: 90000,
  load: { load1: 0.421, load5: 0.37, load15: 0.31, source: 'proc' },
  mem: { rssBytes: 50_000_000, totalBytes: 8_000_000_000, availableBytes: 3_000_000_000 },
}

describe('isFiniteNum', () => {
  it('accepts finite numbers only', () => {
    expect(isFiniteNum(12.3)).toBe(true)
    expect(isFiniteNum(0)).toBe(true)
    expect(isFiniteNum(NaN)).toBe(false)
    expect(isFiniteNum(Infinity)).toBe(false)
    expect(isFiniteNum(undefined)).toBe(false)
    expect(isFiniteNum(null)).toBe(false)
    expect(isFiniteNum('12')).toBe(false)
  })
})

describe('formatCpuPercent', () => {
  it('formats a real reading', () => {
    expect(formatCpuPercent(FULL_TOP)).toBe('12.3%')
  })
  it('answers n/a for the wasm partial payload instead of throwing', () => {
    expect(() => formatCpuPercent(WASM_TOP)).not.toThrow()
    expect(formatCpuPercent(WASM_TOP)).toBe('n/a')
    expect(formatCpuPercent(null)).toBe('n/a')
    expect(formatCpuPercent(undefined)).toBe('n/a')
    expect(formatCpuPercent({ cpuPercent: NaN })).toBe('n/a')
  })
})

describe('formatTopLine', () => {
  it('keeps the unavailable sentence for null', () => {
    expect(formatTopLine(null)).toBe('Native CPU/RAM metrics unavailable in this runtime.')
    expect(formatTopLine(undefined)).toBe('Native CPU/RAM metrics unavailable in this runtime.')
  })
  it('renders the wasm partial payload honestly without throwing', () => {
    let line = ''
    expect(() => { line = formatTopLine(WASM_TOP) }).not.toThrow()
    expect(line).toContain('app CPU n/a')
    expect(line).toContain('system RAM unavailable')
  })
  it('renders the full desktop payload exactly like before', () => {
    expect(formatTopLine(FULL_TOP)).toBe(
      'app CPU 12.3% · app RSS 50.0 MB · RAM 5.0 GB / 8.0 GB · load 0.42',
    )
  })
  it('never throws on hostile shapes', () => {
    expect(() => formatTopLine({} as never)).not.toThrow()
    expect(() => formatTopLine({ mem: null, load: { source: 'proc' } } as never)).not.toThrow()
    expect(formatTopLine({} as never)).toContain('app CPU n/a')
  })
})

describe('formatLoadTriplet', () => {
  it('formats complete readings', () => {
    expect(formatLoadTriplet(FULL_TOP.load)).toBe('0.421 0.37 0.31')
  })
  it('answers n/a for partial or missing loads', () => {
    expect(formatLoadTriplet(null)).toBe('n/a')
    expect(formatLoadTriplet({ source: 'proc' })).toBe('n/a')
    expect(formatLoadTriplet({ load1: 1, load5: undefined, load15: 0 })).toBe('n/a')
  })
})

describe('formatHeapLine', () => {
  it('formats a live heap reading', () => {
    expect(formatHeapLine(true, { usedJSHeapSize: 50_331_648, jsHeapSizeLimit: 2_147_483_648 })).toBe(
      'JS heap 48 MB / 2048 MB',
    )
  })
  it('falls back honestly', () => {
    expect(formatHeapLine(true, null)).toBe('memory: reading…')
    expect(formatHeapLine(false, null)).toBe('memory API not exposed here.')
    expect(formatHeapLine(true, {})).toBe('memory: reading…')
  })
})

describe('formatCoord / formatBytesDec', () => {
  it('guards coordinates', () => {
    expect(formatCoord(-23.55052)).toBe('-23.55052')
    expect(formatCoord(undefined)).toBe('—')
    expect(formatCoord(Infinity)).toBe('—')
  })
  it('guards byte counts', () => {
    expect(formatBytesDec(1_500_000_000)).toBe('1.5 GB')
    expect(formatBytesDec(undefined)).toBe('n/a')
    expect(formatBytesDec(-5)).toBe('n/a')
  })
})
