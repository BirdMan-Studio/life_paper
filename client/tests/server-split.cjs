const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 850 } });
    const errors = [], gameRequests = [], relayRequests = [], tickets = [];
    let offline = false, rejectGame = false, rejectCentral = false, combined = false, empty = false;
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1424/');
    await page.getByRole('button', { name: '连接设置', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '连接设置' });
    await dialog.getByLabel('中心 / 一体服务器地址').fill('https://central.test/api/v1');
    assert.equal(await dialog.locator('input').count(), 1);
    await page.screenshot({ path: `${process.env.TEMP}/life-split-settings-desktop.png` });
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth));
    await page.screenshot({ path: `${process.env.TEMP}/life-split-settings-mobile.png` });
    await dialog.getByRole('button', { name: '保存连接', exact: true }).click();
    const worlds = [{ id: 'world-1', name: 'World one', status: 'active', level_id: 'basic' }, { id: 'world-2', name: 'World two', status: 'paused', level_id: 'basic' }];
    const worldResponse = req => {
      const path = new URL(req.url()).pathname;
      if (req.method() === 'POST' && (path.endsWith('/tutorial') || path.endsWith('/worlds'))) return { data: { id: 'new-world', name: 'Created world', status: 'active' } };
      if (path.endsWith('/worlds')) return { data: worlds };
      if (path.endsWith('/ground')) return { data: {} };
      if (path.endsWith('/organisms') && req.method() === 'POST') {
        assert.equal(req.postDataJSON().model_bundle, 'signed-model-package');
        return { data: { id: 'instance-1' } };
      }
      return { data: [] };
    };
    await page.route('https://central.test/api/v1/**', async route => {
      const req = route.request(), path = new URL(req.url()).pathname;
      if (path.includes('/relay/')) {
        assert.equal(req.headers().authorization, 'Bearer game-1-ticket');
        relayRequests.push({ method: req.method(), path });
        if (path.endsWith('/files')) return route.fulfill({ body: 'raw code' });
        return route.fulfill({ json: worldResponse(req) });
      }
      assert.equal(req.headers().authorization, 'Bearer account-secret');
      if (rejectCentral) return route.fulfill({ status: 401, json: { error: { message: 'account expired' } } });
      if (path.endsWith('/game-servers')) return route.fulfill({ json: { deployment_mode: combined ? 'combined' : 'central', data: empty ? [] : [1, 2].map(n => ({ id: `game-${n}`, name: `Game ${n}`, api_url: `https://game${n}.test/api/v1`, online: true, worlds: [{ id: `world-${n}`, name: `World ${n}` }] })) } });
      if (path.endsWith('/connection')) {
        const id = path.includes('world-2') ? 'game-2' : 'game-1';
        tickets.push(path);
        await new Promise(resolve => setTimeout(resolve, 20));
        return route.fulfill({ json: { data: { game_server_id: id, api_url: `https://${id.replace('-', '')}.test/api/v1`, token: `${id}-ticket`, relay_url: `/api/v1/relay/${id}`, expires_in: 900 } } });
      }
      if (path.endsWith('/export')) return route.fulfill({ json: { data: { model_bundle: 'signed-model-package' } } });
      if (path.endsWith('/player/data')) return route.fulfill({ json: { data: { energy_coins: 0, gold_coins: 0, bio_soup: [] } } });
      if (path.endsWith('/account/me')) return route.fulfill({ json: { data: { username: 'Tester' } } });
      if (path.includes('/worlds')) { assert(combined); return route.fulfill({ json: worldResponse(req) }); }
      return route.fulfill({ json: { data: [] } });
    });
    await page.route(/https:\/\/game[12]\.test\/api\/v1\/.*/, async route => {
      const req = route.request(), url = new URL(req.url()), id = url.hostname === 'game1.test' ? 'game-1' : 'game-2';
      gameRequests.push({ path: url.pathname, method: req.method(), server: id });
      if (offline) return route.abort('failed');
      if (url.pathname.endsWith('/game-server')) {
        assert.equal(req.headers().authorization, undefined);
        return route.fulfill({ json: { data: { server_id: id, auth_mode: 'central', protocol_version: 1 } } });
      }
      assert.equal(req.headers().authorization, `Bearer ${id}-ticket`);
      if (rejectGame) return route.fulfill({ status: 401, json: { error: { message: 'ticket expired' } } });
      if (url.pathname.endsWith('/files')) return route.fulfill({ body: 'raw code' });
      return route.fulfill({ json: worldResponse(req) });
    });
    const run = fn => page.evaluate(fn);
    await run(async () => {
      const api = await import('/src/api/http.js'); api.setToken('account-secret');
      window.unauthorizedEvents = 0; window.addEventListener('life-paper:unauthorized', () => window.unauthorizedEvents++);
      await (await import('/src/stores/data.js')).useDataStore().load();
      if ((await api.worldApi.list()).data.length !== 2) throw new Error('Center world directory missing');
      await api.modelApi.list(); await api.pythonApi.documentation();
    });
    assert.equal(gameRequests.length, 0, 'Lobby/assets/docs never contact game servers');
    assert.equal(tickets.length, 0, 'Lobby does not request game tickets');
    await run(async () => {
      const api = await import('/src/api/http.js'); api.enterWorld('world-1');
      await Promise.all([api.worldApi.ground('world-1'), api.organismApi.list('world-1')]);
      await api.organismApi.create('world-1', 'model-1');
      if (await api.organismApi.program('world-1', 'instance-1').file('', 'main.py') !== 'raw code') throw new Error('Raw transfer failed');
    });
    assert.equal(tickets.length, 1, 'Concurrent world calls share connection ticket');
    rejectGame = true;
    await run(async () => {
      const api = await import('/src/api/http.js');
      try { await api.organismApi.list('world-1'); throw new Error('Expected rejection'); } catch (e) { if (e.status !== 401) throw e; }
      if (window.unauthorizedEvents) throw new Error('Game rejection logged out center');
    });
    rejectGame = false;
    await run(async () => { await (await import('/src/api/http.js')).organismApi.list('world-1'); });
    assert.equal(tickets.length, 2);
    offline = true;
    await run(async () => { await (await import('/src/api/http.js')).organismApi.list('world-1'); });
    assert.equal(relayRequests.length, 1, 'Read retries through relay');
    await run(async () => { const api = await import('/src/api/http.js'); api.leaveWorld(); await api.worldApi.create({ name: 'new' }, 'game-1'); });
    assert.equal(relayRequests.length, 2, 'Creation uses selected server with relay fallback');
    offline = false;
    await run(async () => { const api = await import('/src/api/http.js'); api.setToken('account-secret'); api.enterWorld('world-1'); await api.organismApi.list('world-1'); });
    offline = true;
    const beforeRelay = relayRequests.length, beforeWrites = gameRequests.filter(r => r.method === 'POST').length;
    await run(async () => {
      const api = await import('/src/api/http.js');
      try { await api.worldApi.pause('world-1'); throw new Error('Expected failure'); } catch (e) { if (!e.message.includes('操作结果未知')) throw e; }
      await api.organismApi.list('world-1');
    });
    assert.equal(gameRequests.filter(r => r.method === 'POST').length, beforeWrites + 1);
    assert.equal(relayRequests.length, beforeRelay + 1, 'Write not replayed; following read uses relay');
    offline = false;
    await run(async () => { const api = await import('/src/api/http.js'); api.enterWorld('world-2'); await api.organismApi.list('world-2'); api.leaveWorld(); });
    assert.equal(gameRequests.at(-1).server, 'game-2', 'World switch selects its own server');
    const beforeLeave = gameRequests.length;
    await run(async () => {
      const api = await import('/src/api/http.js');
      try { await api.organismApi.list('world-2'); throw new Error('Expected closed world rejection'); } catch (e) { if (!e.message.includes('请先进入')) throw e; }
      await api.worldApi.list();
    });
    assert.equal(gameRequests.length, beforeLeave, 'Leaving world prevents game requests');
    empty = true;
    await run(async () => { const api = await import('/src/api/http.js'); if ((await api.worldApi.list()).data.length) throw new Error('Expected empty lobby'); });
    assert.equal(gameRequests.length, beforeLeave, 'No game server is a valid empty lobby');
    rejectCentral = true;
    await run(async () => { const api = await import('/src/api/http.js'); try { await api.playerApi.data(); } catch {} if (window.unauthorizedEvents !== 1) throw new Error('Center expiry missing'); });
    rejectCentral = false; combined = true;
    await run(async () => { const api = await import('/src/api/http.js'); api.setToken('account-secret'); await api.worldApi.list(); await api.organismApi.list('world-1'); });
    assert.equal(gameRequests.length, beforeLeave, 'Combined deployment keeps central world API');
    combined = false; empty = false;
    await page.reload();
    await page.getByRole('button', { name: '多人', exact: true }).waitFor();
    assert.equal(gameRequests.length, beforeLeave, 'Hub does not contact game servers');
    await page.getByRole('button', { name: '多人', exact: true }).click();
    await page.locator('.world-list-item').first().waitFor();
    assert.equal(gameRequests.length, beforeLeave, 'Actual login restoration only loads central lobby');
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `Mobile lobby must fit viewport: ${await page.evaluate(() => JSON.stringify({ width: document.documentElement.scrollWidth, bodyMin: getComputedStyle(document.body).minWidth }))}`);
    await page.screenshot({ path: `${process.env.TEMP}/life-central-lobby-mobile.png` });
    await page.locator('.world-list-item').filter({ hasText: 'World 1' }).click();
    await page.locator('.world-back-btn').waitFor();
    await page.waitForFunction(() => window.lifePaperGround !== null && window.lifePaperGround !== undefined);
    assert(gameRequests.length > beforeLeave, 'Actual world entry loads game server data');
    await page.locator('.world-back-btn').click();
    await page.locator('.world-list-item').first().waitFor();
    const requestsAfterBack = gameRequests.length;
    await page.waitForTimeout(1800);
    assert.equal(gameRequests.length, requestsAfterBack, 'Returning to lobby stops game polling');
    await page.setViewportSize({ width: 1280, height: 850 });
    await page.screenshot({ path: `${process.env.TEMP}/life-central-lobby-desktop.png` });
    empty = true;
    await page.getByTitle('刷新世界列表').click();
    await page.getByText('暂无在线世界服务器', { exact: true }).waitFor();
    assert.equal(await page.locator('.world-list-empty .primary-btn').count(), 0, 'Multiplayer does not create local tutorial saves');
    assert.equal(gameRequests.length, requestsAfterBack, 'Refreshing empty lobby stays on center');
    assert.deepEqual(errors, []);
    console.log('PASS: center-only lobby, two game servers, ticket renewal, credentials, relay fallback, write safety, creation, exit guard, empty lobby, combined mode.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
