const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { PNG } = require(process.env.PNG_MODULE || 'pngjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { spawn } = require('node:child_process');

(async () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'life-paper-landscape-'));
  const port = await new Promise(resolve => {
    const listener = net.createServer();
    listener.listen(0, '127.0.0.1', () => { const port = listener.address().port; listener.close(() => resolve(port)); });
  });
  const token = 'landscape-test-local-token-32-characters';
  const config = path.join(temporary, 'game.toml');
  fs.writeFileSync(config, `server_id = "landscape-test"\nhost = "127.0.0.1"\nport = ${port}\ndata_dir = "data"\n[auth]\nmode = "local"\ntoken = "${token}"\n[ai]\nenabled = false\n`);
  const server = spawn(process.env.LANDSCAPE_GAME_SERVER || path.resolve(__dirname, '../../target/debug/game-server.exe'), [], {
    cwd: temporary, env: { ...process.env, LIFE_PAPER_GAME_CONFIG: config }, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  });
  let logs = '';
  server.stdout.on('data', chunk => logs += chunk);
  server.stderr.on('data', chunk => logs += chunk);
  server.on('error', error => logs += error.message);
  let browser;
  try {
    const api = `http://127.0.0.1:${port}/api/v1`;
    let ready = false;
    for (let i = 0; i < 100; i++) {
      try { ready = (await fetch(`${api}/game-server`)).ok; } catch {}
      if (ready) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert(ready, logs);
    const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
    const created = await fetch(`${api}/worlds/tutorial/welcome`, { method: 'POST', headers, body: '{}' });
    assert.equal(created.status, 200, await created.clone().text());
    const { data: world } = await created.json();
    const response = await fetch(`${api}/worlds/${world.id}/ground`, { headers });
    assert.equal(response.status, 200);
    const { data: ground } = await response.json();
    browser = await chromium.launch({ channel: 'msedge', headless: true });
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    await page.route('**/favicon.ico', route => route.fulfill({ status: 204 }));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1425/');
    await page.evaluate(async ground => {
      const source = await (await fetch('/src/components/WorldScene.vue')).text();
      const { createApp, h, ref, nextTick } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Scene } = await import('/src/components/WorldScene.vue');
      const root = document.createElement('div'); document.body.replaceChildren(root);
      document.head.insertAdjacentHTML('beforeend', '<style>body {margin:0; min-width:0} .world-canvas {width:100vw;height:100vh}</style>');
      const scene = ref();
      createApp({ render: () => h(Scene, { ref: scene, ground }) }).mount(root);
      await nextTick(); scene.value.focusPosition({ x: 600, y: 600 });
    }, ground);
    const screenshots = [];
    for (const [name, width, height] of [['desktop', 1100, 700], ['mobile', 390, 740]]) {
      await page.setViewportSize({ width, height });
      await page.waitForTimeout(200);
      const screenshot = path.join(temporary, `${name}.png`);
      const png = PNG.sync.read(await page.locator('canvas').screenshot({ path: screenshot }));
      const terrainOnly = { style: '.map-navigation, .map-scale { visibility: hidden !important; }' };
      const originalTerrain = await page.locator('canvas').screenshot(terrainOnly);
      screenshots.push(screenshot);
      let land = 0, water = 0;
      const colors = new Set();
      for (let i = 0; i < png.data.length; i += 16) {
        const [r, g, b, a] = png.data.subarray(i, i + 4);
        assert.equal(a, 255, 'Terrain canvas must be opaque');
        if (g > r * 1.05 && g > b * 1.1) land++;
        if (b > r * 1.15 && b > g * .85) water++;
        colors.add(`${r},${g},${b}`);
      }
      assert(land > 1000 && water > 1000, `Landscape must show both land and water (${land}, ${water})`);
      assert(colors.size > 300, `Terrain needs surface detail (${colors.size})`);
      const controls = await page.locator('.map-navigation').boundingBox();
      assert(controls.x >= 0 && controls.x + controls.width <= width && controls.y >= 0);
      await page.getByRole('button', { name: '放大', exact: true }).click();
      await page.waitForTimeout(80);
      assert.equal(await page.locator('.map-zoom').textContent(), '140%');
      await page.mouse.move(width / 2, height / 2);
      await page.mouse.down(); await page.mouse.move(width / 2 + 70, height / 2 + 40); await page.mouse.up();
      await page.getByRole('button', { name: '回到初始视野' }).click();
      await page.waitForTimeout(80);
      assert.equal(await page.locator('.map-zoom').textContent(), '100%');
      assert.equal(Buffer.compare(await page.locator('canvas').screenshot(terrainOnly), originalTerrain), 0, 'Reset must restore the original view');
    }
    assert.deepEqual(errors, []);
    console.log('PASS: generated tutorial terrain, desktop/mobile pixels, navigation and reset.');
    console.log(screenshots.join('\n'));
  } finally {
    await browser?.close();
    if (server.exitCode == null) {
      server.kill();
      await new Promise(resolve => server.once('exit', resolve));
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
