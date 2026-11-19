const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const os = require('node:os');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 850 } });
    const errors = [], centralRequests = [], localRequests = [], bridgeCalls = [];
    const catalog = [
      { id: 'welcome', title: '生命起点', description: '认识世界、身体和第一个生物。', order: 10, prerequisites: [], map: { level_id: 'downloaded-welcome', seed: 247, spawn: { x: 600, y: 600 } }, events: [{ id: 'start' }], ui: { on_start: { title: '欢迎来到生命实验场' } } },
      { id: 'matter', title: '物质与容器', description: '学习固体、液体和气体容器。', order: 20, prerequisites: ['welcome'], map: { level_id: 'tutorial_matter' } },
      { id: 'motion', title: '运动与喷射', description: '理解惯性、阻尼和扩散。', order: 30, prerequisites: ['matter'], map: { level_id: 'tutorial_motion' } },
    ];
    const cached = new Map(), saves = [];
    let completed = false, offlineCatalog = false, creations = 0, downloads = 0, starts = 0;
    const world = { id: 'world-welcome', name: '生命起点', level_id: 'downloaded-welcome', tutorial_id: 'welcome', status: 'active' };
    page.on('pageerror', error => errors.push(error.message));
    await page.exposeFunction('localBridge', (command, args) => {
      bridgeCalls.push({ command, args });
      if (command === 'local_world_list') return { data: args.mode === 'tutorial' ? saves.map(save => ({ ...save, tutorial_completed: completed })) : [] };
      if (command === 'local_tutorial_catalog') {
        if (args.entries !== null) cached.set(args.source, args.entries);
        return { data: cached.get(args.source) || [] };
      }
      assert.equal(command, 'local_world_start');
      assert.equal(args.mode, 'tutorial');
      starts++;
      return { api_url: 'http://127.0.0.1:19123/api/v1', token: 'local-secret', server_id: 'local-tutorial' };
    });
    await page.addInitScript(() => {
      localStorage.setItem('life-paper-server', 'https://central.test/api/v1');
      localStorage.setItem('life-paper-token', 'account-secret');
      window.__TAURI_INTERNALS__ = { invoke: (command, args) => window.localBridge(command, args) };
    });
    await page.route('https://central.test/api/v1/**', async route => {
      const req = route.request(), url = new URL(req.url()).pathname;
      centralRequests.push(url);
      assert.equal(req.headers().authorization, 'Bearer account-secret');
      let data = [];
      if (url.endsWith('/account/me')) data = { username: 'Tester' };
      else if (url.endsWith('/player/data')) data = { energy_coins: 0, gold_coins: 0, bio_soup: [] };
      else if (url.endsWith('/tutorials')) {
        if (offlineCatalog) return route.abort('connectionfailed');
        data = catalog;
      } else if (url.endsWith('/tutorials/welcome/package')) {
        assert(!offlineCatalog, 'existing saves must not download packages again');
        downloads++;
        data = { version: 1, definition: catalog[0], world: { ground: { seed: 247 }, terrain_layers: [] } };
      } else assert(!/game-servers|game-worlds|relay|\/worlds/.test(url), `unexpected remote game request: ${url}`);
      return route.fulfill({ json: { data, deployment_mode: 'central' } });
    });
    await page.route('http://127.0.0.1:19123/api/v1/**', async route => {
      const req = route.request(), url = new URL(req.url()).pathname;
      localRequests.push(url);
      if (url.endsWith('/game-server')) {
        assert.equal(req.headers().authorization, undefined);
        return route.fulfill({ json: { data: { server_id: 'local-tutorial', auth_mode: 'local' } } });
      }
      assert.equal(req.headers().authorization, 'Bearer local-secret');
      if (url.endsWith('/tutorial/import')) {
        assert.equal(req.method(), 'POST');
        const pack = req.postDataJSON();
        assert.deepEqual(pack.definition, catalog[0]);
        assert.equal(pack.world.ground.seed, 247);
        creations++;
        saves.push({ ...world, local_mode: 'tutorial', tutorial_definition: catalog[0] });
        return route.fulfill({ json: { data: world } });
      }
      if (url.endsWith('/worlds')) return route.fulfill({ json: { data: [world] } });
      if (url.endsWith('/tutorial')) return route.fulfill({ json: { data: {
        completed, fired_events: completed ? ['start', 'complete'] : ['start'], models: [],
        panels: [{ event_id: 'start', title: '欢迎来到生命实验场', steps: ['创建第一个生物'] }, ...(completed ? [{ event_id: 'complete', message: '基础教程已完成' }] : [])],
      } } });
      return route.fulfill({ json: { data: url.endsWith('/ground') ? {} : [] } });
    });

    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1424/');
    await page.getByRole('button', { name: '教程', exact: true }).waitFor();
    assert.equal(centralRequests.filter(url => /tutorials|game-servers/.test(url)).length, 0);
    assert.equal(localRequests.length, 0);
    await page.getByRole('button', { name: '教程', exact: true }).click();
    await page.getByText('生命起点', { exact: true }).waitFor();
    assert.equal(await page.locator('.tutorial-node').count(), 3);
    assert(await page.getByRole('button', { name: /物质与容器/ }).isDisabled());
    assert.equal(await page.locator('.tutorial-connections path').count(), 2);
    assert.equal(downloads, 0);
    assert.equal(starts, 0);
    assert.equal(localRequests.length, 0);
    await page.screenshot({ path: path.join(os.tmpdir(), 'life-paper-tutorial-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(350);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: path.join(os.tmpdir(), 'life-paper-tutorial-mobile.png') });
    await page.setViewportSize({ width: 1280, height: 850 });
    await page.getByText('生命起点', { exact: true }).click();
    await page.getByRole('region', { name: '教程任务' }).waitFor();
    assert.equal(creations, 1);
    assert.equal(downloads, 1);
    const centralCount = centralRequests.length;
    completed = true;
    await page.getByText('基础教程已完成', { exact: true }).waitFor();
    assert.equal(centralRequests.length, centralCount, 'runtime must only contact local engine');
    await page.locator('.world-back-btn').first().click();
    await page.locator('.tutorial-node').first().waitFor();
    assert(await page.getByRole('button', { name: /物质与容器/ }).isEnabled());
    await page.getByText('生命起点', { exact: true }).click();
    await page.getByRole('region', { name: '教程任务' }).waitFor();
    assert.equal(creations, 1);
    assert.equal(downloads, 1);
    await page.locator('.world-back-btn').first().click();
    await page.locator('.tutorial-node').first().waitFor();
    await page.locator('.world-mode-back').click();
    await page.getByRole('button', { name: '单人', exact: true }).waitFor();
    const localCount = localRequests.length;
    await page.waitForTimeout(1800);
    assert.equal(localRequests.length, localCount);
    await page.getByRole('button', { name: '单人', exact: true }).click();
    await page.getByText('暂无单人存档', { exact: true }).waitFor();
    assert.equal(centralRequests.length, centralCount);

    offlineCatalog = true;
    await page.reload();
    await page.getByRole('button', { name: '教程', exact: true }).click();
    await page.getByText('生命起点', { exact: true }).waitFor();
    assert(await page.getByRole('button', { name: /物质与容器/ }).isEnabled());
    await page.getByText('生命起点', { exact: true }).click();
    await page.getByRole('region', { name: '教程任务' }).waitFor();
    assert.equal(creations, 1);
    assert.equal(downloads, 1);
    assert(!centralRequests.some(url => /game-servers|game-worlds|relay|\/worlds/.test(url)));
    assert(bridgeCalls.some(call => call.command === 'local_tutorial_catalog' && call.args.entries === null));
    assert.deepEqual(errors, []);
    console.log('PASS: central tutorial packages, lazy local import, local-only simulation, credential isolation, completion unlocks, disk catalog fallback and save reuse.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
