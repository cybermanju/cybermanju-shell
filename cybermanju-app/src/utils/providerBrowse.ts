// CyberManju OS — provider browse helpers (pure, unit-tested).
//
// The file manager shows every connected Google Drive / GitHub / GitLab
// repo under `/providers/<mountId>/…`. The canal returns one level with
// folder rows; the native `list_remote_files` REST fallback returns a flat
// recursive file list with no folders — these helpers normalize the
// fallback (synthesize one level of folders) and name the unmounted-config
// placeholders so "all providers" stays visible before mounts exist.

import type { FileNode, RemoteFile } from '@/types'

/** Placeholder mount id for an enabled-but-unmounted sync config. */
export function pendingMountId(configId: string): string {
  return `pending-${String(configId ?? '')}`
}

/** True for a placeholder mount id (no bytes to list until mounted). */
export function isPendingMountId(mountId: string): boolean {
  return String(mountId ?? '').startsWith('pending-')
}

/** Config id behind a placeholder mount id ('' when not a placeholder). */
export function configIdFromPending(mountId: string): string {
  const m = /^pending-(.+)$/.exec(String(mountId ?? ''))
  return m ? m[1] : ''
}

/**
 * Reduce a flat recursive remote listing to one level of children:
 * direct files plus the folders implied by deeper paths. Directory rows
 * sort first, then by name — same order the canal produces.
 */
export function remoteFilesToProviderNodes(
  remote: RemoteFile[],
  mountId: string,
  parentPath: string,
  pathFor: (mountId: string, remotePath: string) => string,
): FileNode[] {
  const now = new Date().toISOString()
  const dirs = new Map<string, FileNode>()
  const out: FileNode[] = []
  for (const r of remote ?? []) {
    const rel = String(r.path ?? r.name ?? '').replace(/^\/+/, '')
    if (!rel) continue
    const slash = rel.indexOf('/')
    if (slash >= 0) {
      const dir = rel.slice(0, slash)
      if (!dir || dirs.has(dir)) continue
      const node = {
        id: `providers/${mountId}/${dir}`,
        name: dir,
        fileType: 'folder',
        parentId: parentPath,
        path: pathFor(mountId, dir),
        sizeBytes: 0,
        encrypted: false,
        compressionLayers: [],
        createdAt: now,
        modifiedAt: r.modifiedAt || now,
      } as FileNode
      dirs.set(dir, node)
      out.push(node)
      continue
    }
    out.push({
      id: `providers/${mountId}/${rel}`,
      name: String(r.name || rel),
      fileType: 'file',
      parentId: parentPath,
      path: pathFor(mountId, rel),
      sizeBytes: Number(r.sizeBytes ?? 0),
      encrypted: false,
      compressionLayers: [],
      createdAt: String(r.modifiedAt || now),
      modifiedAt: String(r.modifiedAt || now),
    } as FileNode)
  }
  out.sort((a, b) => {
    const ad = a.fileType === 'folder' ? 0 : 1
    const bd = b.fileType === 'folder' ? 0 : 1
    return ad - bd || a.name.localeCompare(b.name)
  })
  return out
}
