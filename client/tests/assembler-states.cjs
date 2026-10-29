const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto('http://127.0.0.1:1420/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/components/OrganismAssembler.vue')).text();
      const { createApp, h, ref } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Assembler } = await import('/src/components/OrganismAssembler.vue');
      const components = ['capsule', 'surface', 'surface', 'surface', 'host_outline', 'rectangle', 'ellipse', 'ellipse'].map((geometry, index) => {
        const category = index === 0 ? 'body' : index < 5 ? 'external_organ' : 'internal_organ';
        return { id: `test_${index}`, name: `Component ${index}`, category,
          slots: { counts: { body: 1, external_organ: 4, internal_organ: 3 } },
          model: { geometry, assembly: index === 0 ? 'free' : index === 4 ? 'wrap' : index < 5 ? 'surface' : 'interior',
            size: { width_px: index === 0 ? 40 : index < 5 ? 0 : 12, length_px: index === 0 ? 80 : index < 5 ? 15 : 12 },
            appearance: { fill: '#234339', stroke: '#668e70', stroke_width_px: 2, protrusion_px: 1, range_radius_px: index === 2 ? 5 : null } } };
      });
      const host = document.createElement('div');
      host.className = 'builder-page'; host.style.height = '900px'; document.body.replaceChildren(host);
      const instance = ref(null);
      createApp({ render: () => h(Assembler, { components, ref: instance }) }).mount(host);
      window.assembler = instance;
    });
    for (let index = 0; index < 8; index += 1) {
      await page.evaluate(index => {
        window.assembler.value.reset(); window.assembler.value.addComponent('test_0');
        if (index) window.assembler.value.addComponent(`test_${index}`);
      }, index);
      const id = await page.evaluate(index => window.assembler.value.buildPayload().components.find(c => c.component_id === `test_${index}`).id, index);
      const node = page.locator(`[data-node-id="${id}"]`);
      const label = page.locator(`[data-label-for="${id}"]`);
      const clear = async () => {
        await page.locator('.assembly-canvas').click({ position: { x: 20, y: 20 } });
        await page.mouse.move(10, 10);
      };
      await clear();
      assert.equal(await label.evaluate(e => getComputedStyle(e).opacity), '0');
      const point = await node.evaluate(e => {
        const path = e.querySelector('.surface-hit');
        if (path) { const p = path.getPointAtLength(path.getTotalLength() / 3).matrixTransform(path.getScreenCTM()); return { x: p.x, y: p.y }; }
        const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
      });
      await page.mouse.move(point.x, point.y);
      assert.equal(await label.evaluate(e => getComputedStyle(e).opacity), '1', `hover ${index}`);
      await page.mouse.click(point.x, point.y);
      await page.mouse.move(10, 10);
      assert.equal(await label.evaluate(e => getComputedStyle(e).opacity), '1', `selection ${index}`);
      assert(await node.evaluate(e => e.classList.contains('selected')));
      await clear();
      assert.equal(await label.evaluate(e => getComputedStyle(e).opacity), '0');
      assert((await page.locator('.component-range,.surface-frame').evaluateAll(es => es.map(e => getComputedStyle(e).opacity))).every(x => x === '0'));
    }
    await page.evaluate(() => { window.assembler.value.addComponent('test_4'); });
    await page.screenshot({ path: process.env.ASSEMBLER_SCREENSHOT || 'assembler-states.png' });
    assert.deepEqual(errors, []);
    console.log('PASS: all 8 component fixtures, real pointer hover/selection/clear, shared labels, no idle frames/ranges.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
