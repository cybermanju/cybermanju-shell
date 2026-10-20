// CyberManju OS — relay rooms (src/utils/p2pRoom.ts).
// Role: Trystero-shaped rooms over our own relay mailbox (no new deps) —
// join by `(appId, room)`, post typed actions, read + ack what arrived.
// Browsers that can do UDP get nothing extra here (direct paths stay in
// `wanDial`/`openDataChannelViaRelay`); rooms give every transport,
// including the Pages sandbox, presence + small-data fan-out with zero new
// infrastructure. A Trystero `createStrategy` adapter over the same mailbox
// stays a one-file follow-up if the team ever wants the npm ecosystem.
// Exports: roomTopic, roomHello, postRoomMessage, readRoomMessages, RoomAction.
// Integrates: DevicesPanel room demo; `room` cybsh verb (plan/offline
// builders); relay mailbox clients in p2pPresence.ts.
// Contracts: pure builders throw only `invalid:` (AGENT-1); delivery rides
// the injected relay fetch deps with the mailbox caps; nothing hangs beyond
// the relay's own bounded fetches.

import {
  ackRelayMessages,
  dropRelayMessage,
  pollRelayMailbox,
  type RelayFetchDeps,
  type RelayMessage,
} from './p2pPresence'

/** Room presence heartbeat kind. */
export const ROOM_HELLO_KIND = 'room-hello'

/** Room data action kind. */
export const ROOM_ACTION_KIND = 'room-action'

export interface RoomAction {
  action: string
  body: unknown
  from: string
  atSecs: number
}

/** Mailbox recipient for a room (`room:<appId>/<room>`, both trimmed). */
export function roomTopic(appId: string, room: string): string {
  const app = appId.trim()
  const name = room.trim()
  if (!app) throw new Error('invalid: room needs an app id')
  if (!name) throw new Error('invalid: room needs a room name')
  if (app.length > 120 || name.length > 120) {
    throw new Error('invalid: room app/room names exceed 120 chars')
  }
  if (/[\s@]/.test(app) || /[\s@]/.test(name)) {
    throw new Error('invalid: room names must not contain whitespace or @')
  }
  return `room:${app}/${name}`
}

/** Announce presence in a room (sender id + optional display name). */
export async function roomHello(
  relayBase: string,
  appId: string,
  room: string,
  from: string,
  displayName = '',
  deps: RelayFetchDeps,
): Promise<{ accepted: boolean; id: string }> {
  if (!from.trim()) throw new Error('invalid: room hello needs a sender id')
  return dropRelayMessage(relayBase, from, roomTopic(appId, room), ROOM_HELLO_KIND, {
    displayName,
    atSecs: Math.floor(Date.now() / 1000),
  }, deps)
}

/** Post a typed action to a room. */
export async function postRoomMessage(
  relayBase: string,
  appId: string,
  room: string,
  from: string,
  action: string,
  body: unknown,
  deps: RelayFetchDeps,
): Promise<{ accepted: boolean; id: string }> {
  if (!from.trim()) throw new Error('invalid: room message needs a sender id')
  if (!action.trim()) throw new Error('invalid: room message needs an action')
  return dropRelayMessage(relayBase, from, roomTopic(appId, room), ROOM_ACTION_KIND, {
    action,
    body: body ?? null,
    atSecs: Math.floor(Date.now() / 1000),
  }, deps)
}

/**
 * Read a room's queued messages (hellos + actions from everyone but us),
 * oldest first, acking what was read so redelivery never replays. Returns
 * `{ hellos, actions }` splits for presence vs data rendering.
 */
export async function readRoomMessages(
  relayBase: string,
  appId: string,
  room: string,
  selfId: string,
  deps: RelayFetchDeps,
): Promise<{ hellos: RelayMessage[]; actions: RoomAction[] }> {
  const topic = roomTopic(appId, room)
  const messages = await pollRelayMailbox(relayBase, topic, deps)
  const ids = messages.map((m) => m.id).filter((id) => id.trim())
  if (ids.length > 0) {
    await ackRelayMessages(relayBase, topic, ids, deps).catch(() => 0)
  }
  const hellos: RelayMessage[] = []
  const actions: RoomAction[] = []
  for (const m of messages) {
    if (m.from === selfId) continue
    if (m.kind === ROOM_HELLO_KIND) {
      hellos.push(m)
    } else if (m.kind === ROOM_ACTION_KIND) {
      const payload = (m.payload ?? {}) as Record<string, unknown>
      actions.push({
        action: String(payload.action ?? ''),
        body: payload.body ?? null,
        from: m.from,
        atSecs: Number(payload.atSecs ?? m.createdAtSecs ?? 0) || 0,
      })
    }
  }
  return { hellos, actions }
}
