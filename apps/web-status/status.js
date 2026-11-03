/**
 * DROP IT — public service status page.
 *
 * Static page: no bundler, no dependencies. It reads one public endpoint and
 * renders it:
 *
 *   GET /v1/status/services → { status, checked_at, services: [
 *       { id, name, state, note } ] }
 *
 * This endpoint is deliberately human-facing: it lists the services people
 * use and whether they work, not the internal infrastructure. Rules:
 *   - a valid JSON status document is rendered whatever HTTP code carries it;
 *     today the endpoint always answers 200, and a 502/504 from nginx arrives
 *     as an HTML page — that is a failure, not a document to render;
 *   - a failed read keeps the last successful snapshot on screen and labels it
 *     as stale — it never presents old data as current;
 *   - nothing is invented: a missing name falls back to the id, an unreadable
 *     state becomes `unknown`, a missing note stays empty;
 *   - refreshes every 30 s and stops while the tab is hidden; returning to the
 *     tab reads immediately.
 *
 * All text reaches the DOM through `textContent`; nothing from the endpoint is
 * parsed as HTML.
 */
(() => {
  'use strict';

  const ENDPOINT = '/v1/status/services';
  const REFRESH_MS = 30_000;
  const REQUEST_TIMEOUT_MS = 10_000;

  const STATE_WORDS = {
    ok: 'Operational',
    degraded: 'Degraded',
    down: 'Down',
    unknown: 'Unknown',
  };

  const OVERALL_WORDS = {
    ok: 'All systems operational',
    degraded: 'Some services degraded',
    down: 'Service outage',
    unknown: 'Status unknown',
  };

  const els = {
    refresh: document.getElementById('refresh'),
    refreshLabel: document.getElementById('refresh-label'),
    retry: document.getElementById('retry'),
    overall: document.getElementById('overall'),
    overallText: document.getElementById('overall-text'),
    checked: document.getElementById('checked'),
    banner: document.getElementById('banner'),
    bannerText: document.getElementById('banner-text'),
    checks: document.getElementById('checks'),
    loading: document.getElementById('loading'),
  };

  const state = {
    lastSuccessAt: null,
    inFlight: false,
    timer: null,
  };

  /* --- small DOM helpers -------------------------------------------------- */

  const create = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };

  const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
  const hasState = (value) => typeof value === 'string' && Object.prototype.hasOwnProperty.call(STATE_WORDS, value);

  /** ISO timestamp → `01:40:00` in UTC, or null when it cannot be read. */
  const formatTime = (value) => {
    if (typeof value !== 'string' || value.trim() === '') return null;
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return null;
    try {
      return new Intl.DateTimeFormat('en-GB', {
        timeZone: 'UTC',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false,
      }).format(date);
    } catch {
      return null;
    }
  };

  /* --- rendering ---------------------------------------------------------- */

  /**
   * `stale` marks a snapshot kept on screen after a failed refresh. Without it
   * the biggest word on the page would keep asserting a state nobody could
   * currently confirm.
   */
  const setOverall = (status, stale = false) => {
    const key = hasState(status) ? status : 'unknown';
    const label = stale ? `${OVERALL_WORDS[key]} · stale` : OVERALL_WORDS[key];
    els.overall.dataset.state = key;
    els.overall.dataset.stale = stale ? 'true' : 'false';
    // The pill is a polite live region: only a real change may speak, otherwise
    // every 30-second refresh would repeat the same sentence.
    if (els.overallText.textContent !== label) els.overallText.textContent = label;
  };

  const showBanner = (text) => {
    els.bannerText.textContent = text;
    els.banner.hidden = false;
  };

  const hideBanner = () => {
    els.banner.hidden = true;
    els.bannerText.textContent = '';
  };

  const normalizeService = (raw) => {
    const id = typeof raw.id === 'string' && raw.id.trim() !== '' ? raw.id : 'service';
    const name = typeof raw.name === 'string' && raw.name.trim() !== '' ? raw.name : id;
    const note = typeof raw.note === 'string' ? raw.note : '';
    return { id, name, state: hasState(raw.state) ? raw.state : 'unknown', note };
  };

  const serviceRow = (service) => {
    const row = create('li', 'check');
    row.dataset.state = service.state;

    const dot = create('span', 'check-dot');
    dot.setAttribute('aria-hidden', 'true');

    // The dot is colour; the state also has to exist as words.
    const spoken = create('span', 'sr-only', `${STATE_WORDS[service.state]}. `);
    const label = create('span', 'check-label', service.name);
    const detail = create('span', 'check-detail', service.note);

    row.append(dot, spoken, label, detail);
    return row;
  };

  const renderServices = (rawServices) => {
    const services = [];
    for (const raw of rawServices) {
      if (!isObject(raw) || typeof raw.id !== 'string' || raw.id.trim() === '') continue;
      services.push(normalizeService(raw));
    }

    els.checks.replaceChildren();
    els.loading?.remove();

    if (services.length === 0) {
      els.checks.append(create('p', 'loading', 'The endpoint returned no services.'));
    } else {
      const list = create('ul', 'checks');
      for (const service of services) list.append(serviceRow(service));
      els.checks.append(list);
    }

    els.checks.setAttribute('aria-busy', 'false');
  };

  const renderStatus = (payload) => {
    const status = hasState(payload.status) ? payload.status : 'unknown';
    setOverall(status);

    const checked = formatTime(payload.checked_at);
    if (checked === null) {
      els.checked.textContent = 'The endpoint did not report a check time.';
      els.checked.removeAttribute('title');
    } else {
      els.checked.textContent = `Checked at ${checked} UTC`;
      els.checked.title = typeof payload.checked_at === 'string' ? payload.checked_at : '';
    }

    renderServices(Array.isArray(payload.services) ? payload.services : []);
  };

  const describeFailure = (error, httpStatus) => {
    if (error && error.name === 'AbortError') {
      return `The endpoint did not answer within ${Math.round(REQUEST_TIMEOUT_MS / 1000)} seconds.`;
    }
    if (typeof httpStatus === 'number' && httpStatus >= 400) {
      return `The endpoint answered HTTP ${httpStatus} without a status document.`;
    }
    if (error instanceof TypeError) {
      return 'The request did not reach the server.';
    }
    if (error instanceof Error && error.message) {
      return error.message;
    }
    return 'The status could not be read.';
  };

  /**
   * Keeps the last good snapshot and labels it stale; on the very first read
   * there is nothing to keep, so the page says so instead of showing an empty
   * list that would look like "no services".
   */
  const renderFailure = (message) => {
    showBanner(`Could not refresh the status. ${message}`);
    setOverall('unknown', state.lastSuccessAt !== null);

    if (state.lastSuccessAt === null) {
      els.checked.textContent = 'No successful check yet.';
      els.checks.replaceChildren();
      els.loading?.remove();
      const empty = create('p', 'loading', 'Status unavailable right now. Try again in a moment.');
      els.checks.append(empty);
      els.checks.setAttribute('aria-busy', 'false');
      return;
    }

    const last = formatTime(new Date(state.lastSuccessAt).toISOString());
    els.checked.textContent = last
      ? `Showing the last successful check at ${last} UTC`
      : 'Showing the last successful check';
  };

  /* --- fetching ----------------------------------------------------------- */

  const setBusy = (busy) => {
    els.refresh.disabled = busy;
    // The stylesheet keys the spin animation off `aria-busy` (the state
    // attribute) and not off `data-busy`: without it the control looks idle
    // while a request is actually in flight.
    els.refresh.setAttribute('aria-busy', busy ? 'true' : 'false');
    els.refresh.dataset.busy = busy ? 'true' : 'false';
    els.refreshLabel.textContent = busy ? 'Refreshing…' : 'Refresh';
  };

  const loadStatus = async () => {
    if (state.inFlight) return;
    state.inFlight = true;
    setBusy(true);

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
    let httpStatus;
    try {
      const response = await fetch(ENDPOINT, {
        cache: 'no-store',
        headers: { accept: 'application/json' },
        signal: controller.signal,
      });
      httpStatus = response.status;
      // Read the body even on 4xx/5xx: a status document with an error code is
      // still a document the page can render.
      const text = await response.text();
      let payload = null;
      try {
        payload = JSON.parse(text);
      } catch {
        payload = null;
      }
      if (isObject(payload) && Array.isArray(payload.services)) {
        hideBanner();
        state.lastSuccessAt = Date.now();
        renderStatus(payload);
      } else {
        renderFailure(
          typeof httpStatus === 'number' && httpStatus >= 400
            ? `The endpoint answered HTTP ${httpStatus} without a status document.`
            : 'The endpoint answered with an unexpected document.',
        );
      }
    } catch (error) {
      renderFailure(describeFailure(error, httpStatus));
    } finally {
      clearTimeout(timer);
      state.inFlight = false;
      setBusy(false);
      schedule();
    }
  };

  /** Poll only while the tab is visible. */
  const schedule = () => {
    if (state.timer !== null) clearTimeout(state.timer);
    state.timer = null;
    if (document.hidden) return;
    state.timer = setTimeout(() => {
      state.timer = null;
      void loadStatus();
    }, REFRESH_MS);
  };

  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      if (state.timer !== null) clearTimeout(state.timer);
      state.timer = null;
    } else {
      void loadStatus();
    }
  });

  els.refresh.addEventListener('click', () => void loadStatus());
  els.retry.addEventListener('click', () => void loadStatus());

  void loadStatus();
})();
