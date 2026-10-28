import { chromium } from '../apps/web-club/node_modules/playwright/index.mjs';

const baseURL = process.env.UI_AUDIT_URL ?? 'http://127.0.0.1:5174/dj';
const errors = [];
const runtimeErrors = [];
const report = (message) => {
  errors.push(message);
  console.error(`CRITICAL ERROR: ${message}`);
};
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (error) => runtimeErrors.push(`pageerror: ${error.message}`));
page.on('console', (message) => {
  if (message.type() === 'error') runtimeErrors.push(`console.error: ${message.text()}`);
});

try {
  await page.route('**/v1/auth/dj/login', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ access_token: 'ui-audit-access', refresh_token: 'ui-audit-refresh', token_type: 'Bearer', expires_in: 900 }),
  }));
  const signInForAudit = async () => {
    await page.goto('http://127.0.0.1:5173/dj/login', { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.getByLabel('Email').fill('dj@example.test');
    await page.getByLabel('Password').fill('ui-audit-password');
    await page.getByRole('button', { name: 'Open DJ console' }).click();
    await page.waitForURL((url) => url.pathname === '/dj', { waitUntil: 'domcontentloaded' });
  };
  await page.goto(baseURL, { waitUntil: 'domcontentloaded', timeout: 30000 });
  if (!(await page.locator('.dj-sidebar-link').count())) await signInForAudit();
  await page.waitForTimeout(500);

  const navButtons = page.locator('.dj-sidebar-link');
  const navCount = await navButtons.count();
  if (navCount !== 3) report(`Expected 3 sidebar navigation buttons, found ${navCount}`);
  if (navCount > 0) {
    const widths = await navButtons.evaluateAll((buttons) => buttons.map((button) => ({
      label: button.textContent.trim(), width: button.getBoundingClientRect().width,
    })));
    const range = Math.max(...widths.map((item) => item.width)) - Math.min(...widths.map((item) => item.width));
    if (range > 2) report(`Navigation button widths differ by ${range.toFixed(1)}px: ${widths.map((item) => `${item.label}=${item.width.toFixed(1)}px`).join(', ')}`);
  }

  const truncated = await page.locator('body *').evaluateAll((elements) => elements
    .filter((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none'
        && element.children.length === 0 && element.scrollWidth > element.clientWidth + 2 && element.clientWidth < 120;
    })
    .map((element) => ({ selector: `${element.tagName.toLowerCase()}${element.id ? `#${element.id}` : ''}${element.classList.length ? `.${[...element.classList].join('.')}` : ''}`, text: element.textContent.trim().slice(0, 90), clientWidth: element.clientWidth, scrollWidth: element.scrollWidth })));
  for (const item of truncated) report(`Truncated text ${item.selector} (${item.clientWidth}px/${item.scrollWidth}px): "${item.text}"`);

  const overlaps = await page.evaluate(() => {
    const floating = [...document.querySelectorAll('.dj-toast,[role="tooltip"],[data-floating="true"],footer[style*="position: fixed"]')];
    const buttons = [...document.querySelectorAll('button')].filter((button) => /sign out|accept|reject|pin request/i.test(`${button.textContent} ${button.getAttribute('aria-label')}`));
    const intersect = (a, b) => a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
    return floating.flatMap((overlay) => buttons.filter((button) => overlay !== button && intersect(overlay.getBoundingClientRect(), button.getBoundingClientRect())).map((button) => `${overlay.className || overlay.getAttribute('role')} overlaps ${button.getAttribute('aria-label') || button.textContent.trim()}`));
  });
  overlaps.forEach((item) => report(`Floating element collision: ${item}`));

  const workspace = page.locator('.dj-live-shell main');
  const tabPanels = { 'Request queue': '#dj-queue-panel', 'Room control': '#dj-room-panel', 'Session details': '#dj-session-panel' };
  for (const label of ['Room control', 'Session details', 'Request queue']) {
    await page.locator('.dj-sidebar').getByRole('button', { name: label, exact: true }).click();
    await page.waitForTimeout(250);
    const visiblePanel = await workspace.locator('section[id^="dj-"]').getAttribute('id');
    if (visiblePanel !== tabPanels[label].slice(1)) report(`Dead tab: clicking "${label}" displayed ${visiblePanel ?? 'no panel'}`);
    else console.log(`PASS tab: ${label}`);
  }

  await page.locator('.dj-sidebar').getByRole('button', { name: 'Request queue', exact: true }).click();
  // The queue filter is the panel kit's Select (role=combobox + listbox), not a
  // native <select>: open it, pick the row by its visible name, verify the value.
  const queueFilter = page.locator('#dj-queue-panel [role="combobox"]').first();
  if (await queueFilter.count()) {
    for (const label of ['All active', 'Awaiting decision', 'Accepted']) {
      await queueFilter.click();
      await page.locator('[role="listbox"] [role="option"]', { hasText: label }).first().click();
      if ((await queueFilter.innerText()).trim() !== label) report(`Filter did not activate: ${label}`);
      else console.log(`PASS filter: ${label}`);
    }
  } else {
    for (const label of ['All', 'Awaiting call', 'Queued']) {
      const filter = page.locator('#dj-queue-panel').getByRole('button', { name: label, exact: true });
      if (await filter.count()) { await filter.click(); console.log(`PASS filter: ${label}`); }
      else report(`Missing queue filter: ${label}`);
    }
  }

  for (const label of ['Warm-up', 'Peak time', 'Closing soon']) {
    const button = page.locator('.dj-vibe-controls').getByRole('button', { name: label, exact: true });
    if (await button.count()) {
      await button.click();
      if (!(await button.evaluate((element) => element.classList.contains('is-active')))) report(`Room mode did not activate: ${label}`);
      else console.log(`PASS room mode: ${label}`);
    } else report(`Missing room mode button: ${label}`);
  }

  await page.reload({ waitUntil: 'networkidle' });
  if (!(await page.locator('.dj-sidebar-link').count())) await signInForAudit();
  await page.locator('.dj-sidebar').getByRole('button', { name: 'Request queue', exact: true }).click();
  const pinButton = page.locator('#dj-queue-panel').getByRole('button', { name: 'Pin', exact: true }).first();
  if (await pinButton.count()) {
    const initialQueue = await page.locator('#dj-queue-panel').innerText();
    await pinButton.click();
    await page.waitForTimeout(200);
    const updatedQueue = await page.locator('#dj-queue-panel').innerText();
    if (initialQueue === updatedQueue) report('Pin request did not update queue state');
    else console.log('PASS request action: Pin');
  } else report('Missing request Pin button');

  for (const label of ['Accept', 'Reject']) {
    await page.reload({ waitUntil: 'networkidle' });
    if (!(await page.locator('.dj-sidebar-link').count())) await signInForAudit();
    await page.locator('.dj-sidebar').getByRole('button', { name: 'Request queue', exact: true }).click();
    const button = page.locator('#dj-queue-panel').getByRole('button', { name: label, exact: true }).first();
    if (await button.count()) {
      const before = await page.locator('#dj-queue-panel').innerText();
      await button.click();
      await page.waitForTimeout(200);
      const after = await page.locator('#dj-queue-panel').innerText();
      if (before === after) report(`${label} action produced no visible state update`);
      else console.log(`PASS request action: ${label}`);
    } else report(`Missing request action button: ${label}`);
  }

  runtimeErrors.forEach((error) => report(error));
} catch (error) {
  report(`Auditor failed to complete: ${error.message}`);
} finally {
  await browser.close();
}

console.log(`UI audit completed: ${errors.length} error(s)`);
if (errors.length) process.exitCode = 1;
