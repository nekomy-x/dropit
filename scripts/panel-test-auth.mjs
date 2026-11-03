/** Optional real sign-in for audits of a production build on the test VPS.
 * Credentials stay in a private file, never in argv or screenshots.
 * PANEL_TEST_ACCOUNTS_FILE=deploy/prod/state/<host>-test-accounts.json
 */
import { readFileSync } from 'node:fs';
import { createHmac } from 'node:crypto';

function totp(secret) {
  const bits = [...secret].map((c) => 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c).toString(2).padStart(5, '0')).join('');
  const key = Buffer.from((bits.match(/.{8}/g) ?? []).map((v) => parseInt(v, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)));
  const hash = createHmac('sha1', key).update(counter).digest();
  return ((hash.readUInt32BE(hash[19] & 15) & 0x7fffffff) % 1000000).toString().padStart(6, '0');
}

export async function signInPanelIfNeeded(page, role, requestedRoute) {
  const file = process.env.PANEL_TEST_ACCOUNTS_FILE;
  if (!file || requestedRoute.split('?')[0].endsWith('/login') || !new URL(page.url()).pathname.endsWith('/login')) return;
  const account = JSON.parse(readFileSync(file, 'utf8'))[role];
  if (!account?.email || !account?.password) throw new Error(`Missing private test account for ${role}`);
  await page.locator('input[type=email]').fill(account.email);
  await page.locator('input[type=password]').fill(account.password);
  await page.locator('form button[type=submit]').click();
  if (role === 'admin') {
    const digits = page.locator('.auth-otp-input');
    await digits.first().waitFor();
    const code = totp(account.totp_secret);
    for (let i = 0; i < 6; i++) await digits.nth(i).fill(code[i]);
    await page.locator('form button[type=submit]').click();
  }
  await page.waitForURL((url) => !url.pathname.endsWith('/login'));
  await page.waitForLoadState('networkidle');
}
