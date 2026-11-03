/**
 * Живая проверка стенда 150.241.68.57 (Test Mode, PROD).
 *
 * Что делает: открывает панели в реальном Chromium, проходит регистрацию
 * «в один клик» как клуб и как DJ, логинится демо-аккаунтом, проверяет
 * консоль браузера и делает скриншоты в output/stand-shots/.
 *
 * Запуск: node scripts/verify-stand.mjs [BASE_URL]
 */
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
// Playwright установлен в apps/web-club/node_modules: резолвим оттуда,
// чтобы скрипт работал из корня репозитория, а не только из каталога пакета.
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const BASE = process.argv[2] || 'http://150.241.68.57';
const OUT = process.env.DROPIT_SHOTS || join(REPO, 'output/stand-shots');
mkdirSync(OUT, { recursive: true });

const problems = [];
const shots = [];

function watch(page, label) {
  page.on('console', (msg) => {
    if (msg.type() === 'error') problems.push(`[${label}] console: ${msg.text()}`);
  });
  page.on('pageerror', (err) => problems.push(`[${label}] pageerror: ${err.message}`));
  page.on('response', (res) => {
    const url = res.url();
    if (!url.startsWith(BASE)) return;
    const status = res.status();
    if (status >= 400 && !url.includes('/v1/auth/')) {
      problems.push(`[${label}] HTTP ${status} ${url}`);
    }
  });
}

async function shot(page, name) {
  const path = join(OUT, `${name}.png`);
  await page.screenshot({ path, fullPage: false });
  shots.push(path);
  return path;
}

const browser = await chromium.launch();
const desktop = { viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 };
const mobile = { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2 };

// --- 1. Клубная панель: регистрация в один клик ---------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'club-register');

  await page.goto(`${BASE}/register`, { waitUntil: 'networkidle' });
  await shot(page, '01-register-club-role');

  // Выбор роли «Клуб», затем аккаунт в один клик.
  // Радиоinput — sr-only, реальный клик идёт по <label class="role-option">.
  await page.locator('label.role-option', { hasText: 'Клуб' }).first().click();
  await shot(page, '02-register-club-step2');

  const quick = page.getByRole('button', { name: /один клик/i }).first();
  await quick.click();
  await page.waitForURL(/\/(sessions|dashboard)/, { timeout: 20000 });
  await page.waitForLoadState('networkidle');
  await shot(page, '03-club-sessions-after-signup');
  console.log('club quick signup ->', page.url());
  await ctx.close();
}

// --- 2. DJ: регистрация в один клик --------------------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'dj-register');

  await page.goto(`${BASE}/register?role=dj`, { waitUntil: 'networkidle' });
  await shot(page, '04-register-dj-role');
  await page.getByRole('button', { name: /один клик/i }).first().click();
  await page.waitForURL(/\/dj/, { timeout: 20000 });
  await page.waitForLoadState('networkidle');
  await shot(page, '05-dj-console-after-signup');
  console.log('dj quick signup ->', page.url());
  await ctx.close();
}

// --- 3. Логин демо-клубом и сессии --------------------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'club-login');

  await page.goto(`${BASE}/login`, { waitUntil: 'networkidle' });
  await shot(page, '06-login');
  await page.locator('input[type=email], input[name=email]').first().fill('demo@club.test');
  await page.locator('input[type=password]').first().fill('demo-club-pass');
  await page.locator('button[type=submit]').first().click();
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1500);
  await shot(page, '07-club-after-login');
  console.log('club login ->', page.url());
  await ctx.close();
}

// --- 4. DJ-логин демо-аккаунтом -----------------------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'dj-login');
  await page.goto(`${BASE}/dj/login`, { waitUntil: 'networkidle' });
  await shot(page, '08-dj-login');
  await page.locator('input[type=email], input[name=email]').first().fill('demo@dj.test');
  await page.locator('input[type=password]').first().fill('demo-dj-pass');
  await page.locator('button[type=submit]').first().click();
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1500);
  await shot(page, '09-dj-after-login');
  console.log('dj login ->', page.url());
  await ctx.close();
}

// --- 5. Админка под /admin/ --------------------------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'admin');
  await page.goto(`${BASE}/admin/`, { waitUntil: 'networkidle' });
  await shot(page, '10-admin-root');
  await page.goto(`${BASE}/admin/login`, { waitUntil: 'networkidle' });
  await shot(page, '11-admin-login');
  await page.goto(`${BASE}/admin/payouts`, { waitUntil: 'networkidle' });
  await shot(page, '12-admin-deep-link');
  console.log('admin deep link ->', page.url());
  await ctx.close();
}

// --- 6. DJ-прототип и мобильная ширина ----------------------------------
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'dj-mock');
  await page.goto(`${BASE}/dj-mock/`, { waitUntil: 'networkidle' });
  await shot(page, '13-dj-mock');
  await ctx.close();

  const mctx = await browser.newContext(mobile);
  const mpage = await mctx.newPage();
  watch(mpage, 'mobile');
  await mpage.goto(`${BASE}/register`, { waitUntil: 'networkidle' });
  await shot(mpage, '14-register-mobile');
  const overflow = await mpage.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth + 1,
  );
  if (overflow) problems.push('[mobile] горизонтальный оверфлоу на /register');
  await mctx.close();
}

// --- 7. Публичная страница статуса --------------------------------------
// Страница не хардкодит сервисы: она рендерит ровно то, что вернул
// /v1/status/services. Поэтому проверяем и количество строк, и то, что у
// каждой строки есть валидный data-state — именно по нему красится точка.
{
  const ctx = await browser.newContext(desktop);
  const page = await ctx.newPage();
  watch(page, 'status');
  await page.goto(`${BASE}/status/`, { waitUntil: 'networkidle' });
  // Первичный рендер может завершиться уже после networkidle; ждём с
  // таймаутом и не падаем, чтобы успеть записать проблему и закрыть браузер.
  await page
    .waitForFunction(() => document.querySelectorAll('li.check').length > 0, null, { timeout: 10000 })
    .catch(() => {});
  await shot(page, '15-status');

  const rows = page.locator('li.check');
  const count = await rows.count();
  if (count !== 6) problems.push(`[status] ожидалось 6 строк сервисов, получено ${count}`);

  const states = await rows.evaluateAll((nodes) =>
    nodes.map((node) => node.dataset.state || ''),
  );
  const allowed = ['ok', 'degraded', 'down', 'unknown'];
  states.forEach((state, index) => {
    if (!allowed.includes(state)) {
      problems.push(`[status] строка ${index + 1}: data-state="${state}"`);
    }
  });

  const overall = await page.locator('#overall').getAttribute('data-state');
  if (!allowed.includes(overall)) problems.push(`[status] #overall: data-state="${overall}"`);
  console.log('status ->', states.join(', ') || '(нет строк)');
  await ctx.close();
}

await browser.close();

console.log('\n=== screenshots');
for (const s of shots) console.log(' ', s);
console.log(`\n=== problems: ${problems.length}`);
for (const p of problems) console.log(' !', p);
process.exit(problems.length ? 1 : 0);
