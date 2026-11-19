const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage();
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1425/');
    const result = await page.evaluate(async () => {
      const api = await import('/src/api/http.js');
      const source = await (await fetch('/src/stores/session.js')).text();
      const sessionModule = await import('/src/stores/session.js');
      const sessionApi = await import(source.match(/from\s+["']([^"']*\/api\/http[^"']*)["']/)[1]);
      const session = sessionModule.useSession();
      const nativeFetch = window.fetch;
      const pending = [];
      let events = 0;
      const listener = () => events++;
      window.addEventListener('life-paper:unauthorized', listener);
      window.fetch = (url, options) => new Promise(resolve => pending.push({ url, options, resolve }));
      const started = async () => { while (!pending.length) await new Promise(resolve => setTimeout(resolve, 0)); return pending.shift(); };
      const unauthorized = () => Response.json({ error: { message: 'expired' } }, { status: 401 });
      try {
        for (const change of ['server', 'server-round-trip', 'external-storage']) for (const success of [true, false]) {
          session.clearLocalSession();
          sessionApi.setApiBase('https://login-old.test/api/v1');
          const login = session.login({ username: 'old-account', password: 'test-password' });
          const request = await started();
          if (request.url !== 'https://login-old.test/api/v1/auth/login') throw new Error('Login used the wrong server');
          if (change === 'external-storage') localStorage.setItem('life-paper-server', 'https://login-new.test/api/v1');
          else {
            sessionApi.setApiBase('https://login-new.test/api/v1');
            if (change === 'server-round-trip') sessionApi.setApiBase('https://login-old.test/api/v1');
            sessionApi.setToken('new-session-token'); session.token.value = 'new-session-token'; session.profile.value = { id: 'new-account' };
          }
          request.resolve(success ? Response.json({ data: { token: 'old-server-secret' } }) : unauthorized());
          const result = await Promise.race([login, new Promise(resolve => setTimeout(() => resolve('timeout'), 1000))]);
          if (result !== false || pending.length) throw new Error(`Stale login sent a follow-up request (${change}, success=${success})`);
          const expected = change === 'external-storage' ? '' : 'new-session-token';
          if (sessionApi.getToken() !== expected || session.token.value !== expected) throw new Error('Stale login overwrote the current token');
          if (change !== 'external-storage' && session.profile.value?.id !== 'new-account') throw new Error('Stale login cleared the current profile');
        }
        session.clearLocalSession();
        sessionApi.setApiBase('https://login-current.test/api/v1');
        const accepted = session.login({ username: 'current-account', password: 'test-password' });
        (await started()).resolve(Response.json({ data: { token: 'current-server-secret' } }));
        const profileRequest = await started();
        if (profileRequest.url !== 'https://login-current.test/api/v1/account/me' || profileRequest.options.headers.get('Authorization') !== 'Bearer current-server-secret') throw new Error('Profile request was not bound to its login server');
        profileRequest.resolve(Response.json({ data: { id: 'current-account' } }));
        if (!await accepted || session.profile.value?.id !== 'current-account') throw new Error('Current login failed');
        const delayedProfile = session.login({ username: 'old-account', password: 'test-password' });
        (await started()).resolve(Response.json({ data: { token: 'old-server-secret' } }));
        const staleProfile = await started();
        if (staleProfile.url !== 'https://login-current.test/api/v1/account/me') throw new Error('Profile credentials sent to another server');
        sessionApi.setApiBase('https://login-new.test/api/v1');
        sessionApi.setToken('new-session-token'); session.token.value = 'new-session-token'; session.profile.value = { id: 'new-account' };
        staleProfile.resolve(Response.json({ data: { id: 'old-account' } }));
        if (await delayedProfile || session.profile.value?.id !== 'new-account' || sessionApi.getToken() !== 'new-session-token' || pending.length) throw new Error('Late profile response affected the new session');
        for (const raw of [false, true]) for (const change of ['token', 'server', 'version', 'body']) {
          api.setApiBase('https://old.test/api/v1'); api.setToken('old-token');
          const call = (raw ? api.apiRequestRaw : api.apiRequest)('/account/me').catch(error => error.status);
          const request = await started();
          if (request.options.headers.get('Authorization') !== 'Bearer old-token') throw new Error('Wrong request credentials');
          let releaseBody;
          if (change === 'body' && !raw) {
            const response = unauthorized();
            response.json = () => new Promise(resolve => { releaseBody = resolve; });
            request.resolve(response);
            while (!releaseBody) await new Promise(resolve => setTimeout(resolve, 0));
          }
          if (change === 'server') api.setApiBase('https://new.test/api/v1');
          else if (change === 'version') { api.setToken('new-token'); api.setToken('old-token'); }
          else api.setToken('new-token');
          const before = events;
          if (releaseBody) releaseBody({ error: { message: 'expired' } });
          else request.resolve(unauthorized());
          if (await call !== 401 || events !== before || !api.getToken()) throw new Error(`Stale ${change} response affected current session (raw=${raw})`);
        }
        // The session store also handles 401 directly, independently of the event listener.
        sessionApi.setApiBase('https://current.test/api/v1');
        sessionApi.setToken('old-token'); session.token.value = 'old-token';
        const restore = session.restore();
        const old = await started();
        sessionApi.setToken('new-token'); session.token.value = 'new-token'; session.profile.value = { id: 'new-account' };
        old.resolve(unauthorized());
        await restore;
        if (sessionApi.getToken() !== 'new-token' || session.profile.value?.id !== 'new-account') throw new Error('Stale restore cleared the new account');
        const before = events;
        const current = api.apiRequest('/account/me').catch(error => error.status);
        (await started()).resolve(unauthorized());
        if (await current !== 401 || events !== before + 1) throw new Error('Current session 401 did not expire');
        return true;
      } finally {
        window.fetch = nativeFetch;
        window.removeEventListener('life-paper:unauthorized', listener);
      }
    });
    assert.equal(result, true);
    console.log('PASS: stale login/profile results cannot persist or forward credentials after server changes; stale JSON/raw 401 and restore preserve new sessions; current login/401 still work.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
