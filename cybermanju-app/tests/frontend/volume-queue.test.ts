// CyberManju OS — volume-queue.test (tests/frontend/volume-queue.test.ts).
// Role: pin the single-writer queue (P1-16 frontend half) — FIFO order,
// exactly-once generations, chain survival after failure, `too_large:` gate.
// Exports: none (vitest suite).
// Integrates: src/utils/volumeWriteQueue.ts (+ volumeCaps for the gate).
// Contracts: node-env safe, no window; rejected writes never advance the
// visible order for later writers; over-cap refuses before enqueue.
import { describe, expect, it } from 'vitest'
import { createVolumeWriteQueue } from '@/utils/volumeWriteQueue'
import { VOLUME_WRITE_LIMIT } from '@/utils/volumeCaps'

describe('volume single-writer queue', () => {
  it('serializes concurrent writes in FIFO order with generations', async () => {
    const q = createVolumeWriteQueue()
    const order: number[] = []
    const a = q.enqueue(10, async (slot) => {
      await new Promise((r) => setTimeout(r, 20))
      order.push(slot.generation)
    })
    const b = q.enqueue(10, (slot) => {
      order.push(slot.generation)
    })
    const [sa, sb] = await Promise.all([a, b])
    expect(order).toEqual([sa.generation, sb.generation])
    expect(sb.generation).toBe(sa.generation + 1)
    expect(q.head).toBe(sb.generation)
    await q.drain()
  })

  it('keeps the chain alive after a writer fails', async () => {
    const q = createVolumeWriteQueue()
    await expect(
      q.enqueue(10, () => {
        throw new Error('integrity: boom')
      }),
    ).rejects.toThrow('integrity: boom')
    const slot = await q.enqueue(10, () => {})
    expect(slot.generation).toBe(2)
    await q.drain()
  })

  it('refuses over-cap writes with too_large: before enqueue', async () => {
    const q = createVolumeWriteQueue()
    await expect(q.enqueue(VOLUME_WRITE_LIMIT + 1, () => {})).rejects.toThrow(/^too_large:/)
    expect(q.head).toBe(0)
    const slot = await q.enqueue(VOLUME_WRITE_LIMIT, () => {})
    expect(slot.generation).toBe(1)
  })
})
