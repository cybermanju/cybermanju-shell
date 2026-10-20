// CyberManju OS — Contacts (src/utils/contacts.ts).
// Role: FaceGroups++ (ANALYSIS.md §4.4) — face groups *are* people; this adds
// the contact record (aliases, notes, links to files/places) beside them.
// Merge/dedupe stays the server `merge_face_groups` action; this is the pure
// local record layer persisted to localStorage.
// Exports: ContactRecord, contactFromFaceGroup, addAlias, removeAlias,
// linkFile, unlinkFile, parseContactsStore, serializeContactsStore.
// Integrates: standalone; ArchivePanel PEOPLE tab; store.faceGroups +
// store.mergeFaceGroups for the server-side half.
// Contracts: pure, never throws; aliases trim/dedupe/cap at 20; links
// dedupe (a file links once).
import type { FaceGroup } from '@/types'

export interface ContactRecord {
  groupId: string
  name: string
  aliases: string[]
  notes: string
  /** File ids linked to this person (medoid photo first by convention). */
  links: string[]
  updatedAt: string
}

const MAX_ALIASES = 20
const MAX_LINKS = 200

/** Blank contact shell for a face group (medoid = first member file). */
export function contactFromFaceGroup(group: FaceGroup): ContactRecord {
  return {
    groupId: group.id,
    name: group.name,
    aliases: [],
    notes: '',
    links: [...group.fileIds].slice(0, MAX_LINKS),
    updatedAt: new Date().toISOString(),
  }
}

/** The display photo: explicit first link, else the group's first member. */
export function contactPhoto(contact: ContactRecord, memberIds: string[]): string | null {
  return contact.links[0] ?? memberIds[0] ?? null
}

function touch(contact: ContactRecord): ContactRecord {
  return { ...contact, updatedAt: new Date().toISOString() }
}

/** Add an alias (trimmed, case-insensitive dedupe, capped). */
export function addAlias(contact: ContactRecord, alias: string): ContactRecord {
  const clean = alias.trim()
  if (!clean) return contact
  if (contact.aliases.some((a) => a.toLowerCase() === clean.toLowerCase())) return contact
  if (contact.aliases.length >= MAX_ALIASES) return contact
  return touch({ ...contact, aliases: [...contact.aliases, clean] })
}

export function removeAlias(contact: ContactRecord, alias: string): ContactRecord {
  return touch({ ...contact, aliases: contact.aliases.filter((a) => a !== alias) })
}

export function linkFile(contact: ContactRecord, fileId: string): ContactRecord {
  if (!fileId || contact.links.includes(fileId)) return contact
  if (contact.links.length >= MAX_LINKS) return contact
  return touch({ ...contact, links: [...contact.links, fileId] })
}

export function unlinkFile(contact: ContactRecord, fileId: string): ContactRecord {
  return touch({ ...contact, links: contact.links.filter((id) => id !== fileId) })
}

export function setContactNotes(contact: ContactRecord, notes: string): ContactRecord {
  return touch({ ...contact, notes: notes.slice(0, 4000) })
}

/** Parse the persisted `Record<groupId, ContactRecord>` (garbage → {}). */
export function parseContactsStore(value: unknown): Record<string, ContactRecord> {
  let raw: unknown = value
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return {}
    }
  }
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
  const out: Record<string, ContactRecord> = {}
  for (const [key, entry] of Object.entries(raw as Record<string, unknown>)) {
    if (!entry || typeof entry !== 'object') continue
    const e = entry as Partial<ContactRecord>
    if (typeof e.groupId !== 'string' || typeof e.name !== 'string') continue
    out[key] = {
      groupId: e.groupId,
      name: e.name,
      aliases: Array.isArray(e.aliases) ? e.aliases.filter((a): a is string => typeof a === 'string') : [],
      notes: typeof e.notes === 'string' ? e.notes : '',
      links: Array.isArray(e.links) ? e.links.filter((l): l is string => typeof l === 'string') : [],
      updatedAt: typeof e.updatedAt === 'string' ? e.updatedAt : new Date(0).toISOString(),
    }
  }
  return out
}

export function serializeContactsStore(contacts: Record<string, ContactRecord>): string {
  return JSON.stringify(contacts)
}
