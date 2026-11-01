const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { PNG } = require(process.env.PNG_MODULE || 'pngjs');
const assert = require('node:assert/strict');

const palette = ['#654321', '#0088ff', '#999900', '#005533', '#008000', '#00ffff', '#777777', '#ed4277'];

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.route('**/favicon.ico', route => route.fulfill({ status: 204 }));
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    await page.evaluate(async palette => {
      const source = await (await fetch('/src/components/WorldScene.vue')).text();
      const { createApp, h, ref } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Scene } = await import('/src/components/WorldScene.vue');
      const keys = ['cliff', 'pond', 'valley', 'swamp', 'grassland', 'wetland', 'wall', 'wasteland'];
      const host = document.createElement('div'); document.body.replaceChildren(host);
      const nativeRaf = window.requestAnimationFrame.bind(window);
      window.terrainFrames = 0;
      window.requestAnimationFrame = callback => nativeRaf(time => { window.terrainFrames++; callback(time); });
      window.testGround = ref({
        generator: { noise_scale_px: 384, variation_percent: 100, area: { shape: 'rectangle', origin: { x: 0, y: 0 }, size_x_px: 150, size_y_px: 150 }, profile: Object.fromEntries(keys.map(key => [key, key])) },
        perlin_permutations: Array.from({ length: 4 }, (_, c) => Array.from({ length: 256 }, (_, i) => (i * 73 + c * 19) & 255)),
        default_terrain: 'wasteland',
        terrain_layers: [{ terrain: 'pond', area: { shape: 'circle', center: { x: 0, y: 0 }, radius_px: 20 } }],
        terrains: keys.map((id, i) => ({ id, visual: { color: palette[i] } })),
      });
      createApp({ render: () => h(Scene, { ground: window.testGround.value }) }).mount(host);
      Object.assign(host.firstElementChild.style, { width: '640px', height: '384px' });
      window.dispatchEvent(new Event('resize'));
    }, palette);
    const canvas = page.locator('canvas');
    for (const zoom of [1, .25, null]) {
      let burstFrame;
      if (zoom === .25) {
        burstFrame = await page.evaluate(() => window.terrainFrames);
        const elapsed = await page.locator('.world-canvas').evaluate(element => {
          const start = performance.now();
          for (let i = 0; i < 20; i++) element.dispatchEvent(new WheelEvent('wheel', { deltaY: 100, cancelable: true }));
          return performance.now() - start;
        });
        console.log('20 zoom events main-thread dispatch (ms):', elapsed);
      }
      if (zoom === null) await page.evaluate(() => { window.testGround.value = { ...window.testGround.value, generator: null, perlin_permutations: [] }; });
      await page.waitForTimeout(100);
      if (burstFrame != null) assert.equal(await page.evaluate(() => window.terrainFrames), burstFrame + 1, 'A burst of wheel events must draw only once');
      assert.deepEqual(errors, []);
      const png = PNG.sync.read(await canvas.screenshot({ path: `terrain-${zoom}.png` }));
      const expectedPixels = await page.evaluate(async zoom => {
        const { createTerrainSampler } = await import('/src/rendering/terrain.js');
        const sample = createTerrainSampler(window.testGround.value);
        const colors = Object.fromEntries(window.testGround.value.terrains.map(item => [item.id, parseInt(item.visual.color.slice(1), 16)]));
        const result = [];
        for (let y = 3; y < 380; y += 5) for (let x = 3; x < 636; x += 5) {
          const wx = (x + .5 - 320) * 320 / 384 / (zoom || .25), wy = (y + .5 - 192) * 320 / 384 / (zoom || .25);
          const color = colors[sample(Math.floor(wx), Math.floor(wy)).key];
          result.push([color >> 16, (color >> 8) & 255, color & 255]);
        }
        return result;
      }, zoom);
      let checked = 0, matches = 0;
      let pixel = 0;
      for (let y = 3; y < 380; y += 5) for (let x = 3; x < 636; x += 5) {
        const expected = expectedPixels[pixel++];
        const worldX = (x + .5 - 320) * 320 / 384 / (zoom || .25);
        const worldY = (y + .5 - 192) * 320 / 384 / (zoom || .25);
        if (Math.abs(worldX - Math.round(worldX)) < .01 || Math.abs(worldY - Math.round(worldY)) < .01) continue;
        const i = (y * png.width + x) * 4;
        checked++;
        if (expected.every((value, c) => Math.abs(value - png.data[i + c]) <= 1)) matches++;
      }
      assert(matches / checked > .999, `GPU terrain must match CPU at zoom ${zoom}: ${matches}/${checked}`);
    }
    const before = await page.evaluate(() => window.terrainFrames);
    await page.waitForTimeout(200);
    assert.equal(await page.evaluate(() => window.terrainFrames), before, 'Idle scene must not redraw');
    console.log('PASS: full-detail GPU terrain matches CPU at default/minimum zoom; idle rendering stops.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
