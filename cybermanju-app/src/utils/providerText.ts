// CyberManju OS — ProviderText (src/utils/providerText.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: MAX_PROVIDER_TEXT_BYTES, isTextEditableProviderName, decodeProviderText.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
export const MAX_PROVIDER_TEXT_BYTES = 1024 * 1024

const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'rst', 'csv', 'tsv', 'log', 'json', 'jsonc', 'yaml', 'yml', 'toml', 'ini', 'env',
  'html', 'htm', 'css', 'scss', 'less', 'js', 'mjs', 'cjs', 'ts', 'tsx', 'jsx', 'vue', 'svelte', 'xml', 'svg',
  'rs', 'py', 'sh', 'bash', 'zsh', 'fish', 'c', 'h', 'cc', 'cpp', 'hpp', 'java', 'kt', 'go', 'rb', 'php',
  'sql', 'graphql', 'gql', 'dockerfile', 'makefile', 'conf', 'config', 'properties', 'lock', 'gitignore', 'editorconfig',
])

const TEXT_FILENAMES = new Set([
  '.env', '.gitignore', '.gitattributes', '.editorconfig', 'dockerfile', 'makefile', 'license', 'readme',
])

export function isTextEditableProviderName(name: string, mimeType = ''): boolean {
  const base = String(name ?? '').replace(/\\/g, '/').split('/').pop()?.toLowerCase() ?? ''
  if (!base || /\.cybermanju(?:$|\.)|\.cyb3$|\.cybe1$/i.test(base)) return false
  if (TEXT_FILENAMES.has(base)) return true
  const extension = base.includes('.') ? base.split('.').pop() ?? '' : ''
  return TEXT_EXTENSIONS.has(extension) || String(mimeType).toLowerCase().startsWith('text/')
}

export function decodeProviderText(bytes: Uint8Array): string {
  if (bytes.byteLength > MAX_PROVIDER_TEXT_BYTES) {
    throw new Error(`too_large: text editing is limited to ${MAX_PROVIDER_TEXT_BYTES} bytes`)
  }
  const header = String.fromCharCode(...bytes.subarray(0, 8))
  if (/^(?:CYBMJ(?:01|U1)|CYBE1|CYB3)/.test(header)) {
    throw new Error('binary_container: .cybermanju and encrypted containers must be opened by the vault, not edited as text')
  }
  if (bytes.includes(0)) throw new Error('binary_file: this file contains binary bytes')
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
  } catch {
    throw new Error('invalid_text: file is not valid UTF-8 text')
  }
}
