const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const instances = { first: [], second: [] };
    const programs = {};
    let runs = 0;
    let manualRuns = 0;
    let failSave = false;
    let failLogs = false;
    await page.route('**/api/v1/worlds/*/organisms/*/*', async route => {
      const request = route.request();
      const url = new URL(request.url());
      const parts = url.pathname.split('/');
      assert.equal(parts.at(-4), 'first');
      const id = parts.at(-2);
      if (parts.at(-1) === 'ai') {
        if (failLogs) return route.fulfill({ status: 500, json: { error: { message: 'Logs unavailable' } } });
        return route.fulfill({ json: { data: { enabled: true, state: { calls: runs, last_run: runs ? {
          ok: runs === 1, logs: runs === 1 ? 'hello organism\n' : '', error: runs === 1 ? null : 'ValueError: test failure', duration_ms: 12,
        } : null } } } });
      }
      programs[id] ||= { 'main.py': 'speed = 1\n', 'brain.py': 'turn = 0\n' };
      if (parts.at(-1) === 'folder') return route.fulfill({ json: { data: Object.keys(programs[id]).map(path => ({ name: path, path, kind: 'file', size: programs[id][path].length })) } });
      assert.equal(parts.at(-1), 'files');
      const path = url.searchParams.get('path');
      if (request.method() === 'PUT') {
        if (failSave) return route.fulfill({ status: 500, json: { error: { message: 'Save failed' } } });
        programs[id][path] = request.postData();
        return route.fulfill({ status: 204 });
      }
      return route.fulfill({ body: programs[id][path] });
    });
    await page.route('**/api/v1/worlds/*/organisms/*/ai/run', async route => {
      manualRuns++;
      return route.fulfill({ status: 409 });
    });
    let posts = 0;
    await page.route('**/api/v1/worlds/*/organisms', async route => {
      const request = route.request();
      const worldId = new URL(request.url()).pathname.split('/').at(-2);
      if (request.method() === 'POST') {
        posts++;
        assert.deepEqual(request.postDataJSON(), { model_id: 'model' });
        await new Promise(resolve => setTimeout(resolve, 150));
        const organism = { id: `instance-${posts}`, name: 'Server organism', model_id: 'model', position: { x: 720, y: -480 }, rotation_mdeg: 0,
          composition: { root_body: 'body', components: { body: { id: 'body', component: 'body', placement: { position: { x: 0, y: 0 }, size: { width_px: 20, length_px: 40 } } } } },
          component_states: { body: { durability: 100, energy_mj: 0, container: { non_gas_capacity_mg: 2000, gas_capacity_ul: 4000, non_gas_contents_mg: {}, gas_contents_ul: {} } } }, ai: { calls: 0, memory: {}, last_run: null } };
        instances[worldId].push(organism);
        return route.fulfill({ status: 201, json: { data: organism } });
      }
      return route.fulfill({ json: { data: instances[worldId] } });
    });
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/views/WorldView.vue')).text();
      const { createApp, h } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: WorldView } = await import('/src/views/WorldView.vue');
      const sceneSource = await (await fetch('/src/components/WorldScene.vue')).text();
      const { useDataStore } = await import(sceneSource.match(/from\s+["']([^"']*\/stores\/data\.js[^"']*)["']/)[1]);
      const { setApiBase } = await import('/src/api/http.js');
      setApiBase(`${location.origin}/api/v1`);
      useDataStore().allComponents.value = [{ id: 'body', category: 'body', model: { geometry: 'capsule', size: { width_px: 20, length_px: 40 }, appearance: { fill: '#ff0000', stroke: '#ffffff' } } }];
      const root = document.createElement('div'); document.body.replaceChildren(root);
      document.body.style.minWidth = '0';
      createApp({ render: () => h(WorldView, {
        worlds: ['first', 'second'].map(id => ({ id, name: id, level_id: 'test', status: 'active' })),
        models: [{ id: 'model', name: 'Test model' }], ground: {},
      }) }).mount(root);
    });
    await page.locator('.world-list-item').filter({ hasText: 'first' }).click();
    const organismsCheckbox = page.getByRole('checkbox', { name: '生物层', exact: true });
    const matterCheckbox = page.getByRole('checkbox', { name: '汤层', exact: true });
    assert(await organismsCheckbox.isChecked());
    assert(await matterCheckbox.isChecked());
    await matterCheckbox.uncheck();
    assert(await organismsCheckbox.isChecked());
    await organismsCheckbox.uncheck();
    assert(!(await matterCheckbox.isChecked()));
    await organismsCheckbox.check();
    await matterCheckbox.check();
    const add = page.getByTitle('添加生物', { exact: true });
    await add.click();
    const create = page.locator('.organism-picker button');
    await create.click();
    assert(await create.isDisabled(), 'Creation must disable duplicate submissions');
    await page.locator('.world-creature-row').waitFor();
    assert.equal(await page.locator('.world-creature-row strong').textContent(), 'Server organism');
    assert.equal(posts, 1);
    await page.locator('.world-creature-row').click();
    assert.equal(await page.locator('.code-editor').count(), 0, 'Only the explicit enter action opens the instance');
    const firstRow = page.locator('.world-creature-row').first();
    assert.equal(await firstRow.locator('button').count(), 3);
    const canvas = page.locator('.world-canvas');
    const box = await canvas.boundingBox();
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    async function tileAt(offset = 0) {
      await page.mouse.move(center.x + offset, center.y);
      return (await page.locator('.tile-inspector b').textContent()).match(/\((-?\d+), (-?\d+)\)/).slice(1).map(Number);
    }
    await page.mouse.move(center.x, center.y);
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(100);
    const before = await tileAt();
    const scale = (await tileAt(50))[0] - before[0];
    await organismsCheckbox.uncheck();
    await firstRow.getByTitle('定位生物', { exact: true }).click();
    assert(await organismsCheckbox.isChecked());
    const located = await tileAt();
    assert(Math.abs(located[0] - 720) <= 1 && Math.abs(located[1] + 480) <= 1, 'Locate must preserve world Y direction');
    assert(Math.abs((await tileAt(50))[0] - located[0] - scale) <= 1, 'Locate must preserve zoom');
    await firstRow.getByTitle('查看日志', { exact: true }).click();
    await page.getByText('暂无运行记录', { exact: true }).waitFor();
    failLogs = true;
    await page.getByTitle('刷新日志', { exact: true }).click();
    await page.getByRole('alert').filter({ hasText: 'Logs unavailable' }).waitFor();
    failLogs = false;
    await page.getByTitle('刷新日志', { exact: true }).click();
    await page.getByText('暂无运行记录', { exact: true }).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('dialog').count(), 0);
    await firstRow.getByTitle('进入实例', { exact: true }).click();
    await page.locator('.code-editor').waitFor();
    instances.first[0].component_states.body.energy_mj = 999;
    await page.waitForFunction(() => document.querySelector('.component-information-text')?.value.includes('999 mJ'));
    instances.first[0].component_states.body.energy_mj = 998;
    await page.waitForFunction(() => document.querySelector('.component-information-text')?.value.includes('998 mJ'));
    assert.equal(await page.locator('.code-editor').inputValue(), 'speed = 1\n');
    assert.equal(await page.locator('.detail-actions .danger-btn').count(), 0);
    const mainRow = page.locator('.file-row').filter({ hasText: 'main.py' });
    assert(await mainRow.locator('.file-actions button').first().isDisabled());
    assert(await mainRow.locator('.file-actions button').nth(1).isDisabled());
    await page.locator('.code-editor').fill('speed = 2\n');
    await Promise.all([
      page.waitForResponse(response => response.request().method() === 'PUT'),
      page.locator('.code-actions button').first().click(),
    ]);
    assert.equal(programs['instance-1']['main.py'], 'speed = 2\n');
    await page.locator('.detail-toolbar > button').click();
    await page.locator('.world-creature-row').getByTitle('进入实例', { exact: true }).click();
    await page.locator('.code-editor').waitFor();
    assert.equal(await page.locator('.code-editor').inputValue(), 'speed = 2\n');
    await page.locator('.detail-toolbar > button').click();
    await add.click();
    await page.locator('.organism-picker button').click();
    await page.locator('.world-creature-row').nth(1).waitFor();
    await page.locator('.world-creature-row').nth(1).getByTitle('进入实例', { exact: true }).click();
    await page.locator('.code-editor').waitFor();
    assert.equal(await page.locator('.code-editor').inputValue(), 'speed = 1\n');
    assert.equal(programs['instance-1']['main.py'], 'speed = 2\n');
    await page.locator('.detail-toolbar > button').click();
    await page.getByTitle('返回世界列表', { exact: true }).click();
    await Promise.all([
      page.waitForResponse(response => response.url().endsWith('/worlds/second/organisms')),
      page.locator('.world-list-item').filter({ hasText: 'second' }).click(),
    ]);
    assert.equal(await page.locator('.world-creature-row').count(), 0);
    await page.getByTitle('返回世界列表', { exact: true }).click();
    await page.locator('.world-list-item').filter({ hasText: 'first' }).click();
    await page.locator('.world-creature-row').first().waitFor();
    assert.equal(await page.locator('.world-creature-row').count(), 2);
    await page.locator('.world-creature-row').first().getByTitle('进入实例', { exact: true }).click();
    await page.locator('.code-editor').fill('def main(game):\n    game.log("hello organism")\n');
    const run = page.getByTitle('运行 AI', { exact: true });
    assert.equal(await run.count(), 0, 'Instance details must not offer manual AI execution');
    await Promise.all([
      page.waitForResponse(response => response.request().method() === 'PUT'),
      page.locator('.code-actions button').first().click(),
    ]);
    assert(programs['instance-1']['main.py'].includes('def main(game)'));
    runs = 1; // Simulate an automatic world tick completing.
    if (process.env.AI_SCREENSHOT) await page.screenshot({ path: process.env.AI_SCREENSHOT, fullPage: true });
    for (const width of [1024, 390, 1280]) {
      await page.setViewportSize({ width, height: 800 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `Editor must fit width ${width}`);
    }
    await page.locator('.detail-toolbar > button').click();
    await firstRow.getByTitle('查看日志', { exact: true }).click();
    await page.locator('.organism-log-output').filter({ hasText: 'hello organism' }).waitFor();
    await page.screenshot({ path: `${process.env.TEMP}/life-world-logs.png`, fullPage: true });
    await page.getByTitle('关闭', { exact: true }).click();
    await firstRow.getByTitle('进入实例', { exact: true }).click();
    await page.locator('.code-editor').waitFor();
    assert.equal(await run.count(), 0);
    failSave = true;
    await Promise.all([
      page.waitForResponse(response => response.status() === 500),
      page.locator('.code-actions button').first().click(),
    ]);
    await page.getByRole('status').filter({ hasText: 'Save failed' }).waitFor();
    runs = 2;
    await page.locator('.detail-toolbar > button').click();
    await firstRow.getByTitle('查看日志', { exact: true }).click();
    await page.locator('.organism-log-error').filter({ hasText: 'ValueError: test failure' }).waitFor();
    await page.keyboard.press('Escape');
    for (const width of [1280, 390]) {
      await page.setViewportSize({ width, height: 800 });
      await firstRow.locator('strong').evaluate(node => { node.textContent = 'A very long organism name '.repeat(8); });
      await page.waitForFunction(() => document.documentElement.scrollWidth <= innerWidth);
      const overflow = await page.evaluate(() => [...document.querySelectorAll('*')].filter(node => node.getBoundingClientRect().right > innerWidth + 1).map(node => `${node.tagName}.${node.className}`).slice(0, 12));
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `World must fit width ${width}: ${overflow}`);
      await page.screenshot({ path: `${process.env.TEMP}/life-world-actions-${width}.png`, fullPage: true });
    }
    assert.deepEqual(errors, []);
    assert.equal(manualRuns, 0, 'Editing and saving must never execute AI manually');
    console.log('PASS: three organism actions, camera position and zoom, log refresh and errors, responsive layout, independent AI editing and world isolation (mock HTTP).');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
