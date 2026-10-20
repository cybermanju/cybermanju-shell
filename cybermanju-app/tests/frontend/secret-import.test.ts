// CyberManju OS — SecretImport.test (tests/frontend/secret-import.test.ts).
// §4.1: 1Password / Bitwarden / generic CSV parsing.
import { describe, expect, it } from 'vitest'
import { parseSecretCsv } from '../../src/utils/secretImport'

const BITWARDEN = `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp
Email,0,login,Gmail,my notes,,0,https://mail.example.com,ada@example.com,s3cret,
`;

const ONEPASSWORD = `Title,Username,Password,URL,Notes
Bank,ada,money123,https://bank.example.com,VIP
,Lonely,,,
`;

describe('parseSecretCsv', () => {
  it('detects Bitwarden and maps login_* columns', () => {
    const res = parseSecretCsv(BITWARDEN);
    expect(res.format).toBe('bitwarden');
    expect(res.errors).toEqual([]);
    expect(res.rows).toHaveLength(1);
    expect(res.rows[0].input).toMatchObject({
      kind: 'login',
      title: 'Gmail',
      username: 'ada@example.com',
      value: 's3cret',
      url: 'https://mail.example.com',
    });
    expect(res.rows[0].input.tags).toContain('imported');
  });

  it('detects 1Password and skips untitled rows', () => {
    const res = parseSecretCsv(ONEPASSWORD);
    expect(res.format).toBe('1password');
    expect(res.rows).toHaveLength(1);
    expect(res.skipped).toBe(1);
    expect(res.rows[0].input.title).toBe('Bank');
  });

  it('marks passwordless notes as notes', () => {
    const res = parseSecretCsv(`name,notes\nWifi code,front door 1234\n`);
    expect(res.rows[0].input.kind).toBe('note');
    expect(res.rows[0].input.value).toBeNull();
  });

  it('refuses garbage and oversized files', () => {
    expect(parseSecretCsv('').errors).toHaveLength(1);
    const big = `title,password\n${'a,b\n'.repeat(2001)}`;
    const res = parseSecretCsv(big);
    expect(res.rows).toHaveLength(0);
    expect(res.errors[0]).toMatch(/too_large/);
  });
});
