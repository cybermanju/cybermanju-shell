// CyberManju OS — Volume single-writer queue (src/utils/volumeWriteQueue.ts).
// Role: frontend half of the P1-16 single-writer design (ANALYSIS.md §2C) —
// serialize whole-blob volume writes (localStorage JSON vs Rust vs mirror)
// through one promise chain with generation counters, so concurrent callers
// never interleave a read-modify-write race. The Rust single-opener design
// stays sequenced; this removes the browser-side racer today.
// Exports: VolumeWriteQueue, createVolumeWriteQueue.
// Integrates: useVolumeMirror / staticCybsh volume paths enqueue here;
// cap enforcement reuses volumeCaps (1 MiB, `too_large:`); tested in
// tests/frontend/volume-queue.test.ts (node env, no window).
// Contracts: FIFO, exactly-once per enqueue; a rejected write never advances
// the generation; over-cap writes refuse with `too_large:` before touching
// the queue; pure in-memory — persistence stays with the volume/mirror.
import { VOLUME_WRITE_LIMIT } from './volumeCaps'

export interface QueuedWrite {
  /** Monotonic generation assigned at enqueue (starts at 1). */
  generation: number
  /** Byte length the writer declared. */
  bytes: number
}

export class VolumeWriteQueue {
  private tail: Promise<void> = Promise.resolve()
  private generation = 0

  /** Depth of enqueued-but-unsettled writes (0 = idle). */
  get pending(): number {
    return this.inflight
  }

  private inflight = 0

  /** Current generation (advances only on accepted writes). */
  get head(): number {
    return this.generation
  }

  /**
   * Enqueue `write` behind every earlier write. Over-cap sizes refuse with
   * `too_large:` without touching the chain; failures reject the caller but
   * never break the chain for later writers.
   */
  enqueue(bytes: number, write: (slot: QueuedWrite) => Promise<void> | void): Promise<QueuedWrite> {
    if (bytes > VOLUME_WRITE_LIMIT) {
      return Promise.reject(
        new Error(`too_large: queued write is ${bytes} bytes, volume write limit is ${VOLUME_WRITE_LIMIT}`),
      )
    }
    this.generation += 1
    const slot: QueuedWrite = { generation: this.generation, bytes }
    this.inflight += 1
    const run = this.tail.then(async () => {
      await write(slot)
      return slot
    })
    // Keep the chain alive for later writers regardless of outcome.
    this.tail = run.then(
      () => undefined,
      () => undefined,
    )
    return run.finally(() => {
      this.inflight -= 1
    })
  }

  /** Drain the chain (resolves when every write so far has settled). */
  drain(): Promise<void> {
    return this.tail
  }
}

/** Factory (one queue per volume substrate). */
export function createVolumeWriteQueue(): VolumeWriteQueue {
  return new VolumeWriteQueue()
}

/** Shared browser volume queue; every production writer must use this instance. */
export const volumeWriteQueue = createVolumeWriteQueue()

/** Run a result-producing volume mutation in the shared FIFO sequence. */
export async function withVolumeWriteQueue<T>(bytes: number, write: () => Promise<T> | T): Promise<T> {
  let result!: T
  await volumeWriteQueue.enqueue(bytes, async () => {
    result = await write()
  })
  return result
}
