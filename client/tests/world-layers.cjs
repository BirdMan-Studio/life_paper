const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const { PNG } = require(process.env.PNG_MODULE || 'pngjs');

function assertVerticalShift(before, after, deltaY) {
  const a = PNG.sync.read(before); const b = PNG.sync.read(after);
  let matches = 0; let total = 0;
  for (let y = 8; y < a.height - 8; y += 3) {
    const targetY = y + deltaY;
    if (targetY < 8 || targetY >= b.height - 8) continue;
    for (let x = 8; x < a.width - 8; x += 3) {
      const source = (y * a.width + x) * 4; const target = (targetY * b.width + x) * 4;
      if (a.data[source] === b.data[target] && a.data[source + 1] === b.data[target + 1] && a.data[source + 2] === b.data[target + 2]) matches++;
      total++;
    }
  }
  assert(matches / total > .99, `Terrain must follow vertical drag ${deltaY}px across redraws (${matches}/${total})`);
}

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/components/WorldScene.vue')).text();
      const { createApp, h, ref } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: WorldScene } = await import('/src/components/WorldScene.vue');
      const { useDataStore } = await import(source.match(/from\s+["']([^"']*\/stores\/data\.js[^"']*)["']/)[1]);
      useDataStore().allComponents.value = [{
        id: 'body', category: 'body', model: {
          geometry: 'capsule', size: { width_px: 20, length_px: 40 },
          appearance: { fill: '#ff0000', stroke: '#ffffff' },
        },
      }];
      const organisms = ref([]);
      const showOrganisms = ref(true), showMatter = ref(true);
      window.worldTest = { organisms, showOrganisms, showMatter };
      const ground = {
        generator: { seed: 1, profile: Object.fromEntries(['cliff', 'pond', 'valley', 'swamp', 'grassland', 'wetland', 'wall'].map(id => [id, id])) },
        terrains: ['cliff', 'pond', 'valley', 'swamp', 'grassland', 'wetland', 'wall'].map((id, i) => ({ id, visual: { color: ['#654321', '#0088ff', '#999900', '#005533', '#008000', '#00ffff', '#777777'][i] } })),
      };
      const host = document.createElement('div');
      document.body.replaceChildren(host);
      createApp({ render: () => h(WorldScene, { ground, showOrganisms: showOrganisms.value, showMatter: showMatter.value, organisms: organisms.value, onTileHover: tile => { window.worldTest.tile = tile; } }) }).mount(host);
    });
    for (const viewport of [{ width: 1280, height: 800 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await page.evaluate(() => { window.worldTest.organisms.value = []; });
      await page.waitForTimeout(150);
      const canvas = page.locator('canvas');
      const base = await canvas.screenshot();
      await page.evaluate(() => {
        window.worldTest.organisms.value = [{ position: { x: 0, y: 0 }, rotation_mdeg: 0, composition: {
          root_body: 'root', components: { root: { id: 'root', component: 'body', placement: { position: { x: 100, y: 100 }, size: { width_px: 20, length_px: 40 } } } },
        } }];
      });
      await page.waitForTimeout(150);
      const overlay = await canvas.screenshot();
      assert(!base.equals(overlay), 'Adding a structured organism must change the map');
      await page.evaluate(() => { window.worldTest.showOrganisms.value = false; });
      await page.waitForTimeout(100);
      assert(base.equals(await canvas.screenshot()), 'Hiding organisms must leave ground visible');
      await page.evaluate(() => { window.worldTest.showMatter.value = false; });
      await page.waitForTimeout(100);
      assert(base.equals(await canvas.screenshot()), 'Disabling both overlays must preserve ground');
      await page.evaluate(() => { window.worldTest.showOrganisms.value = true; });
      await page.waitForTimeout(100);
      assert(overlay.equals(await canvas.screenshot()), 'Organisms must be independent of matter visibility');
      await page.evaluate(() => { window.worldTest.showMatter.value = true; });
      await page.waitForTimeout(100);
      assert(overlay.equals(await canvas.screenshot()), 'Enabling matter must preserve the organism overlay');
      await page.evaluate(() => { window.worldTest.organisms.value = []; });
      await page.waitForTimeout(150);
      assert(base.equals(await canvas.screenshot()), 'Removing organisms must restore the ground map');
    }
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.locator('.world-canvas').evaluate(element => { element.style.width = '640px'; element.style.height = '384px'; });
    await page.evaluate(() => window.dispatchEvent(new Event('resize')));
    const canvas = page.locator('canvas');
    const box = await canvas.boundingBox();
    const x = box.x + 320; const y = box.y + 192;
    await page.mouse.move(x, y);
    const startTile = await page.evaluate(() => window.worldTest.tile);
    await page.waitForTimeout(100);
    const initial = await canvas.screenshot();
    // Large enough to cross the cached window boundary in both directions.
    for (const delta of [96, -96, -96, 96]) {
      const before = await canvas.screenshot();
      await page.mouse.move(x, y); await page.mouse.down();
      await page.mouse.move(x, y + delta, { steps: 12 }); await page.mouse.up();
      await page.waitForTimeout(100);
      assertVerticalShift(before, await canvas.screenshot(), delta);
    }
    await page.mouse.move(x, y);
    assert.equal(JSON.stringify(await page.evaluate(() => window.worldTest.tile)), JSON.stringify(startTile));
    assert(initial.equals(await canvas.screenshot()), 'Returning to the same camera position must restore identical terrain');
    await page.mouse.wheel(0, -100); await page.mouse.wheel(0, 100);
    await page.waitForTimeout(100);
    assert.deepEqual(errors, []);
    console.log('World overlays and bidirectional vertical dragging across redraws verified');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
