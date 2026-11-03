#!/usr/bin/env node
/** Browser regression check; the provider and API are fixtures, never charges.
 * Start Vite with VITE_GUEST_STRIPE_PUBLISHABLE_KEY=pk_test_checkout_fixture.
 * GUEST_CHECKOUT_URL=http://127.0.0.1:5185 node scripts/guest-checkout-check.mjs
 */
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const require = createRequire(new URL('../apps/web-club/package.json', import.meta.url));
const { chromium } = require('playwright');
const base = process.env.GUEST_CHECKOUT_URL ?? 'http://127.0.0.1:5185';
const output = fileURLToPath(new URL('../output/playwright/guest-checkout/', import.meta.url));
mkdirSync(output, { recursive: true });
const token = 'ab'.repeat(32);
const track = { id: '019a0000-0000-7000-8000-000000000001', title: 'Move', artist: 'Adam Port, Stryv' };
const sessionId = '019a0000-0000-7000-8000-000000000002';
const requestId = '019a0000-0000-7000-8000-000000000003';
const intent = { payment_id: '019a0000-0000-7000-8000-000000000004', client_secret: 'pi_checkout_secret_fixture', amount_minor: 300, currency: 'EUR', status: 'intent' };
const provider = `window.Stripe = function () { return {
  _registerWrapper() {}, registerAppInfo() {},
  elements() { return { create() { let ready; return {
    on(event, fn) { if(event === 'ready') ready = fn; },
    mount(node) { if(typeof node === 'string') node = document.querySelector(node); node.textContent = 'Card or wallet · test provider'; setTimeout(() => ready && ready(), 0); },
    destroy() {},
  }; } }; },
  async confirmPayment(options) { window.providerCalls = (window.providerCalls || 0) + 1; window.lastReturnUrl = options.confirmParams.return_url;
    if(window.decline) return {error: {message: 'Card declined · test provider'}};
    return {paymentIntent: {id: 'pi_checkout', status: 'requires_capture'}};
  },
  async retrievePaymentIntent() { return { paymentIntent: { id: 'pi_checkout', status: window.providerStatus || 'requires_capture' } }; },
}; };`;
const browser = await chromium.launch({ headless: true });
let assertions = 0;
async function test(name, run) {
  if (process.argv.includes('--test-method-only') && !name.startsWith('test method')) return;
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  const state = { requests: [], intents: [], confirms: [], testConfirms: [], testPayments: false, failIntent: false, failConfirm: false, confirmStatus: 200, joinStatus: 200, roomStatus: 'active' };
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  await page.addInitScript(() => { localStorage.setItem('dropit.language', 'ru'); localStorage.setItem('dropit-theme', 'dark'); });
  await page.route('https://js.stripe.com/**', (route) => route.fulfill({ contentType: 'text/javascript', body: provider }));
  await page.route('**/v1/**', async (route) => {
    const req = route.request(); const path = new URL(req.url()).pathname;
    const json = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
    if (path === '/v1/status') return json({ mode: 'normal', source: 'adaptive', payments: 'live', open_signup: false });
    if (path === '/v1/guest/device') return json({ device_token: '019a0000-0000-7000-8000-000000000005' });
    assert.ok(req.headers().authorization, `guest auth on ${path}`);
    if (path === '/v1/sessions/join') return state.joinStatus === 200
      ? json({ test_payments: state.testPayments, session_id: sessionId, club_name: 'Kater Blau · test fixture', dj_display_name: 'DJ Markus', status: state.roomStatus, price_minor: 300 })
      : json({ code: 'not_found', message: 'QR expired' }, state.joinStatus);
    if (path === '/v1/catalog/search') return json({ items: [track] });
    assert.ok(req.headers()['idempotency-key'], `idempotency on ${path}`);
    if (path === '/v1/requests') { state.requests.push(req.headers()['idempotency-key']); return json({ request_id: requestId, session_id: sessionId, status: 'pending' }, 201); }
    if (path.endsWith('/payment')) {
      state.intents.push(req.headers()['idempotency-key']);
      if (state.failIntent) { state.failIntent = false; return route.abort(); }
      return json({ ...intent, test_mode: state.testPayments, client_secret: state.testPayments ? 'pi_mock_fixture_secret_mock' : intent.client_secret }, 201);
    }
    if (path.endsWith('/test-confirm')) {
      state.testConfirms.push(req.headers()['idempotency-key']);
      if (state.failConfirm) { state.failConfirm = false; return route.abort(); }
      if (state.confirmStatus !== 200) return json({ code: 'session_paused', message: 'Room paused' }, state.confirmStatus);
      return json({ payment_id: intent.payment_id, status: 'captured', test_mode: true });
    }
    if (path.endsWith('/confirm')) {
      state.confirms.push(req.headers()['idempotency-key']);
      if (state.failConfirm) { state.failConfirm = false; return route.abort(); }
      if (state.confirmStatus !== 200) return json({ code: 'conflict', message: 'Payment state changed' }, state.confirmStatus);
      return json({ payment_id: intent.payment_id, status: 'processing' });
    }
    throw new Error(`Unmocked API: ${path}`);
  });
  try {
    await run(page, state);
    assert.deepEqual(pageErrors, [], 'no browser exceptions');
    console.log(`PASS ${name}`); assertions++;
  } finally { await context.close(); }
}
async function openRoom(page) {
  await page.goto(`${base}/join?token=${token}&app=web`);
  await page.getByText('Kater Blau · test fixture', { exact: true }).waitFor();
}
async function selectTrack(page) {
  await page.getByRole('searchbox').fill('Move');
  await page.getByRole('button', { name: /Move Adam Port/ }).click();
}
async function startPayment(page) {
  await selectTrack(page);
  await page.getByRole('button', { name: 'Перейти к оплате' }).click();
  await page.getByText('Card or wallet · test provider').waitFor();
  await page.getByRole('checkbox').check();
}
try {
  await test('test method: same order, no Stripe SDK, explicit no-charge receipt', async (page, state) => {
    state.testPayments = true;
    let stripeRequests = 0;
    page.on('request', (request) => { if (request.url().includes('js.stripe.com')) stripeRequests++; });
    await openRoom(page); await selectTrack(page);
    await page.getByRole('button', { name: 'Перейти к оплате' }).click();
    await page.getByRole('group', { name: 'Тестовая оплата' }).waitFor();
    await page.getByRole('checkbox').check();
    await page.screenshot({ path: `${output}/test-payment-dark-mobile.png`, fullPage: true });
    for (const theme of ['light', 'dark']) {
      await page.evaluate((value) => document.documentElement.dataset.theme = value, theme);
      for (const width of [320, 390, 1440]) {
        await page.setViewportSize({ width, height: 844 });
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
        if (width !== 320) await page.screenshot({ path: `${output}/test-payment-${theme}-${width}.png`, fullPage: true });
      }
    }
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole('button', { name: /Оплатить тестово/ }).dblclick();
    await page.getByText('Тестовая оплата выполнена · без списания').waitFor();
    assert.equal(state.requests.length, 1); assert.equal(state.testConfirms.length, 1);
    assert.equal(state.confirms.length, 0); assert.equal(stripeRequests, 0);
    await page.reload(); await page.getByText('Тестовая оплата выполнена · без списания').waitFor();
    assert.equal(state.testConfirms.length, 1);
  });
  await test('test method lost confirmation: retry the same payment without creating a second request', async (page, state) => {
    state.testPayments = true; state.failConfirm = true;
    await openRoom(page); await selectTrack(page);
    await page.getByRole('button', { name: 'Перейти к оплате' }).click();
    await page.getByRole('group', { name: 'Тестовая оплата' }).waitFor();
    await page.getByRole('checkbox').check();
    await page.getByRole('button', { name: /Оплатить тестово/ }).click();
    await page.getByRole('alert').waitFor(); await page.reload();
    await page.getByRole('button', { name: 'Продолжить оплату' }).click();
    await page.getByText('Тестовая оплата выполнена · без списания').waitFor();
    assert.equal(state.requests.length, 1); assert.equal(new Set(state.testConfirms).size, 1);
  });
  await test('test method rejects a paused-session conflict without contacting Stripe', async (page, state) => {
    state.testPayments = true; state.confirmStatus = 409;
    let stripeRequests = 0;
    page.on('request', (request) => { if (request.url().includes('js.stripe.com')) stripeRequests++; });
    await openRoom(page); await selectTrack(page);
    await page.getByRole('button', { name: 'Перейти к оплате' }).click();
    await page.getByRole('group', { name: 'Тестовая оплата' }).waitFor();
    await page.getByRole('checkbox').check();
    await page.getByRole('button', { name: /Оплатить тестово/ }).click();
    await page.getByRole('alert').waitFor();
    assert.equal(await page.getByRole('heading', { name: 'Ваш запрос принят.' }).count(), 0);
    assert.equal(stripeRequests, 0);
  });
  await test('no QR: valid manual entry, no sign-in gate', async (page) => {
    await page.goto(`${base}/join`);
    await page.getByRole('textbox').fill('invalid');
    await page.getByRole('button', { name: 'Войти в комнату' }).click();
    await page.getByRole('alert').waitFor();
    await page.getByRole('textbox').fill(`${base}/join?token=${token}`);
    await page.getByRole('button', { name: 'Войти в комнату' }).click();
    await page.getByText('Kater Blau · test fixture', { exact: true }).waitFor();
  });
  await test('expired QR: recoverable error', async (page, state) => {
    state.joinStatus = 404;
    await page.goto(`${base}/join?token=${token}`);
    await page.getByText(/Этот QR больше не действует/).waitFor();
    state.joinStatus = 200;
    await page.getByRole('button', { name: 'Повторить' }).click();
    await page.getByText('Kater Blau · test fixture', { exact: true }).waitFor();
  });
  await test('paused room: no payment', async (page, state) => {
    state.roomStatus = 'paused'; await openRoom(page); await selectTrack(page);
    assert.equal(await page.getByRole('button', { name: 'Перейти к оплате' }).isDisabled(), true);
    assert.equal(state.requests.length, 0);
    state.roomStatus = 'active'; await page.reload(); await startPayment(page);
    state.roomStatus = 'paused'; await page.reload();
    await page.getByText('Card or wallet · test provider').waitFor();
    await page.getByRole('checkbox').check();
    assert.equal(await page.getByRole('button', { name: /Оплатить/ }).isDisabled(), true);
    assert.equal(state.confirms.length, 0);
  });
  await test('room, search, payment, receipt and no duplicate on double click', async (page, state) => {
    await openRoom(page); await startPayment(page);
    await page.screenshot({ path: `${output}/payment-dark-mobile-fixture.png`, fullPage: true });
    await page.getByRole('button', { name: /Оплатить/ }).dblclick();
    await page.getByRole('heading', { name: 'Ваш запрос принят.' }).waitFor();
    assert.equal(state.requests.length, 1); assert.equal(state.intents.length, 1); assert.equal(state.confirms.length, 1);
    assert.equal(await page.evaluate(() => window.providerCalls), 1);
    assert.equal(new URL(await page.evaluate(() => window.lastReturnUrl)).pathname, '/guest/payment-return');
    assert.ok(await page.getByText(requestId).isVisible());
    await page.reload(); await page.getByRole('heading', { name: 'Ваш запрос принят.' }).waitFor();
    assert.equal(state.requests.length, 1);
  });
  await test('lost intent response: same request and idempotency key', async (page, state) => {
    state.failIntent = true; await openRoom(page); await selectTrack(page);
    await page.getByRole('button', { name: 'Перейти к оплате' }).click();
    await page.getByRole('alert').waitFor();
    await page.reload(); await page.getByRole('button', { name: 'Продолжить оплату' }).click();
    await page.getByText('Card or wallet · test provider').waitFor();
    assert.equal(state.requests.length, 1); assert.equal(state.intents.length, 2);
    assert.equal(new Set(state.intents).size, 1);
  });
  await test('lost confirmation: retry never authorizes the card twice', async (page, state) => {
    state.failConfirm = true; await openRoom(page); await startPayment(page);
    await page.getByRole('button', { name: /Оплатить/ }).click();
    await page.getByRole('alert').waitFor();
    await page.getByRole('button', { name: 'Продолжить оплату' }).click();
    await page.getByRole('heading', { name: 'Ваш запрос принят.' }).waitFor();
    assert.equal(await page.evaluate(() => window.providerCalls), 1);
    assert.equal(state.confirms.length, 2); assert.equal(new Set(state.confirms).size, 1);
  });
  await test('declined card: no success and no backend confirmation', async (page, state) => {
    await openRoom(page); await startPayment(page); await page.evaluate(() => { window.decline = true; });
    await page.getByRole('button', { name: /Оплатить/ }).click();
    await page.getByText('Card declined · test provider').waitFor();
    assert.equal(state.confirms.length, 0);
    assert.equal(await page.getByRole('heading', { name: 'Ваш запрос принят.' }).count(), 0);
  });
  await test('capture webhook wins confirmation race: verify paid intent', async (page, state) => {
    state.confirmStatus = 409; await openRoom(page); await startPayment(page);
    await page.evaluate(() => { window.providerStatus = 'succeeded'; });
    await page.getByRole('button', { name: /Оплатить/ }).click();
    await page.getByRole('heading', { name: 'Ваш запрос принят.' }).waitFor();
    assert.equal(await page.evaluate(() => window.providerCalls), 1);
  });
  await test('session guard conflict is not reported as a paid request', async (page, state) => {
    state.confirmStatus = 409; await openRoom(page); await startPayment(page);
    await page.getByRole('button', { name: /Оплатить/ }).click();
    await page.getByRole('alert').waitFor();
    assert.equal(await page.getByRole('heading', { name: 'Ваш запрос принят.' }).count(), 0);
  });
  await test('3DS return: verify provider, confirm once, clean secret from URL', async (page, state) => {
    await page.addInitScript(({ token, track, sessionId, requestId, intent }) => {
      sessionStorage.setItem('dropit.guest.checkout', JSON.stringify({ key: 'web-redirect-fixture', roomToken: token, sessionId, requestId, track, intent }));
    }, { token, track, sessionId, requestId, intent });
    await page.goto(`${base}/guest/payment-return?token=${token}&app=web&payment_intent_client_secret=${intent.client_secret}&redirect_status=succeeded`);
    await page.getByRole('heading', { name: 'Ваш запрос принят.' }).waitFor();
    assert.equal(state.requests.length, 0); assert.equal(state.confirms.length, 1);
    assert.ok(!page.url().includes('client_secret'));
  });
  await test('layout: both themes, phone and desktop, no horizontal overflow', async (page) => {
    await openRoom(page); await selectTrack(page);
    for (const theme of ['dark', 'light']) {
      await page.evaluate((value) => document.documentElement.dataset.theme = value, theme);
      for (const width of [320, 390, 768, 1440]) {
        await page.setViewportSize({ width, height: 844 });
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, `${theme}@${width}`);
        if ([390, 1440].includes(width)) await page.screenshot({ path: `${output}/search-${theme}-${width}-fixture.png`, fullPage: true });
      }
    }
  });
  console.log(`${assertions} guest checkout scenarios passed. Screenshots: ${output}`);
} finally { await browser.close(); }
