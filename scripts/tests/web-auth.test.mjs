// Run with Node >= 22.18: node --test scripts/tests/web-auth.test.mjs
import assert from 'node:assert/strict';
import { describe, test } from 'node:test';

const pair = (id) => ({ access_token: `access-${id}`, refresh_token: `refresh-${id}`, expires_in: 900, token_type: 'Bearer' });
const json = (body, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
const unauthorized = () => json({ code: 'unauthorized', message: 'Unauthorized' }, 401);
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
let counter = 0;

for (const app of ['web-admin', 'web-club']) {
  describe(app, () => {
    async function setup(t) {
      const client = await import(new URL(`../../apps/${app}/src/api/client.ts?test=${counter++}`, import.meta.url));
      const read = app === 'web-admin' ? client.getDashboardNow : client.getClubDashboard;
      let handler;
      t.mock.method(globalThis, 'fetch', (...args) => handler(...args));
      const mock = (next) => { handler = next; };
      const login = async (id) => {
        mock(async () => json(pair(id)));
        await client.login({ email: 'test@example.test', password: 'test-only' });
      };
      return { client, read, mock, login };
    }

    test('parallel 401s share one refresh rotation', async (t) => {
      const { read, mock, login } = await setup(t);
      await login('old');
      const refresh = deferred();
      let rotations = 0;
      mock(async (url, init) => {
        if (url === '/v1/auth/refresh') {
          rotations++;
          return refresh.promise;
        }
        return init.headers.Authorization === 'Bearer access-old' ? unauthorized() : json({});
      });
      const results = Promise.all([read(), read(), read()]);
      await new Promise(setImmediate);
      assert.equal(rotations, 1);
      refresh.resolve(json(pair('new')));
      await results;
      assert.equal(rotations, 1);
    });

    test('a delayed old-token 401 replays with the rotated token', async (t) => {
      const { read, mock, login } = await setup(t);
      await login('old');
      const late = deferred();
      let requests = 0;
      let rotations = 0;
      mock(async (url, init) => {
        if (url === '/v1/auth/refresh') {
          rotations++;
          return json(pair('new'));
        }
        if (init.headers.Authorization === 'Bearer access-old') {
          return ++requests === 1 ? unauthorized() : late.promise;
        }
        return json({});
      });
      const first = read();
      const second = read();
      await first;
      late.resolve(unauthorized());
      await second;
      assert.equal(rotations, 1);
    });

    test('logout prevents an in-flight refresh from restoring auth', async (t) => {
      const { client, mock, login } = await setup(t);
      await login('old');
      const refresh = deferred();
      mock(async () => refresh.promise);
      const result = client.refreshTokens();
      client.clearTokens();
      refresh.resolve(json(pair('late')));
      await assert.rejects(result, { status: 401 });
      assert.equal(client.isAuthenticated(), false);
    });

    test('an old request cannot sign out a newly logged-in account', async (t) => {
      const { client, read, mock, login } = await setup(t);
      await login('old');
      const old = deferred();
      mock(async () => old.promise);
      const result = read();
      await login('new-account');
      mock(async () => unauthorized());
      old.resolve(unauthorized());
      await assert.rejects(result, { status: 401 });
      assert.equal(client.isAuthenticated(), true);
    });

    test('failed public login does not clear an existing session', async (t) => {
      const { client, mock, login } = await setup(t);
      await login('current');
      mock(async () => unauthorized());
      await assert.rejects(client.login({ email: 'other@example.test', password: 'wrong' }), { status: 401 });
      assert.equal(client.isAuthenticated(), true);
    });
  });
}
