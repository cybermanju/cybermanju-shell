// CyberManju OS — SecretImport (src/utils/secretImport.ts).
// Role: password-manager CSV import (§4.1 follow-up) — 1Password / Bitwarden /
// generic exports → SecretInput rows for the sealed keystore. Pure parse;
// the panel loops store.createSecret (values sealed server-side, Admin
// posture unchanged).
// Exports: ImportFormat, ImportedSecret, parseSecretCsv.
// Integrates: csv.ts parser; SecretsPanel import modal; SecretInput type.
// Contracts: pure, never throws; caps at 2000 rows; rows without a title
// are skipped (counted); passwords stay in memory only until saved.
import type { SecretInput, SecretKind } from '@/types'
import { parseDelimited } from './csv'

export type ImportFormat = 'bitwarden' | '1password' | 'generic'

export interface ImportedSecret {
  input: SecretInput
  /** 1-based CSV line number (for error messages). */
  line: number
}

export interface ImportResult {
  format: ImportFormat
  rows: ImportedSecret[]
  skipped: number
  errors: string[]
}

const MAX_ROWS = 2000

function col(row: Record<string, string>, ...names: string[]): string {
  for (const name of names) {
    const hit = Object.keys(row).find((k) => k.trim().toLowerCase() === name);
    if (hit && row[hit].trim() !== '') return row[hit].trim();
  }
  return '';
}

function detectFormat(headers: string[]): ImportFormat {
  const lower = headers.map((h) => h.trim().toLowerCase());
  if (lower.some((h) => h.startsWith('login_'))) return 'bitwarden';
  if (lower.includes('title') && lower.includes('username') && lower.includes('password')) {
    return '1password';
  }
  return 'generic';
}

/** Parse a password-manager CSV export into keystore inputs. */
export function parseSecretCsv(text: string): ImportResult {
  const errors: string[] = [];
  const table = parseDelimited(text);
  if (!table || table.columns.length === 0) {
    return { format: 'generic', rows: [], skipped: 0, errors: ['empty or unreadable CSV'] };
  }
  if (table.rows.length > MAX_ROWS) {
    return {
      format: 'generic',
      rows: [],
      skipped: 0,
      errors: [`too_large: ${table.rows.length} rows exceeds the ${MAX_ROWS}-row import cap`],
    };
  }
  const format = detectFormat(table.columns);
  const rows: ImportedSecret[] = [];
  let skipped = 0;
  table.rows.forEach((cells, i) => {
    const record: Record<string, string> = {};
    table.columns.forEach((c, j) => {
      record[c] = cells[j] ?? '';
    });
    const title = col(record, 'title', 'name', 'account', 'item');
    const username = col(record, 'login_username', 'username', 'user', 'login', 'email');
    const password = col(record, 'login_password', 'password', 'pass', 'passwd');
    const url = col(record, 'login_uri', 'url', 'website', 'site', 'link');
    const notes = col(record, 'notes', 'note', 'comments', 'memo');
    const category = col(record, 'folder', 'category', 'group', 'vault');
    if (!title) {
      skipped += 1;
      return;
    }
    let kind: SecretKind = 'login';
    if (!password && !username && notes) kind = 'note';
    rows.push({
      line: i + 2,
      input: {
        kind,
        title: title.slice(0, 200),
        username: username || null,
        url: url || null,
        category: category || null,
        tags: ['imported'],
        notes: notes || null,
        favorite: false,
        value: password || null,
      },
    });
  });
  return { format, rows, skipped, errors };
}
