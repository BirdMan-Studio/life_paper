const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const os = require('node:os');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/views/ModelDetailView.vue')).text();
      const { createApp, h, ref } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Detail } = await import('/src/views/ModelDetailView.vue');
      const { useDataStore } = await import('/src/stores/data.js');
      const ids = ['b59ce770-015e-421e-9043-8877e15da001', 'b59ce770-015e-421e-9043-8877e15da002', 'b59ce770-015e-421e-9043-8877e15da003', 'b59ce770-015e-421e-9043-8877e15da004'];
      const properties = [
        { mass_mg: 40000, energy_capacity_mj: 1000, non_gas_capacity_mg: 2000, gas_capacity_ul: 4000, reaction_min_mj_per_tick: -10, reaction_max_mj_per_tick: 2 },
        { non_gas_capacity_mg: 20000, reaction_min_mj_per_tick: -50, reaction_max_mj_per_tick: 10 },
        { mass_mg: 5000 }, { mass_mg: 2000 },
      ];
      const definitions = [
        ['small_body_1', '小型躯体I', 'body', 'capsule', 'free', 40, 80, '#234339', '#668e70'],
        ['reaction_structure_1', '反应结构I', 'internal_organ', 'rectangle', 'interior', 8, 12, '#cc6633', '#ffffff'],
        ['jet_organ_1', '喷射器官I', 'external_organ', 'surface', 'surface', 0, 15, 'none', '#59d9e6'],
        ['skin_1', '皮肤I', 'external_organ', 'host_outline', 'wrap', 0, 15, 'none', '#7eab86'],
      ].map(([id, name, category, geometry, assembly, width_px, length_px, fill, stroke], index) => ({
        id, name, category,
        properties: { max_durability: 100, damage_weight: 10, repair_weight: 10, mass_mg: null, energy_capacity_mj: 0, non_gas_capacity_mg: 0, gas_capacity_ul: 0, reaction_min_mj_per_tick: 0, reaction_max_mj_per_tick: 0, sensor_slots: 0, ...properties[index] },
        model: { geometry, assembly, size: { width_px, length_px }, appearance: { fill, stroke, stroke_width_px: 1, protrusion_px: assembly === 'surface' ? 3 : 0 } },
      }));
      useDataStore().allComponents.value = definitions;
      const model = ref({ id: 'model-1', name: '测试生物模型', updated_at: '2026-11-01T08:00:00Z', composition: {
        root_body: ids[0], components: Object.fromEntries(ids.map((id, i) => [id, {
          id, component: definitions[i].id, placement: {
            ...(i === 0 ? { position: { x: 100, y: -30 } } : { host_body: ids[0] }),
            ...(i === 2 ? { surface_anchor: { perimeter_ppm: 125000 } } : {}),
            size: definitions[i].model.size, rotation_mdeg: 0,
          },
        }])),
      } });
      const instanceMode = ref(false);
      const api = { folder: async () => ({ data: [] }) };
      const root = document.createElement('div'); document.body.replaceChildren(root);
      document.body.style.minWidth = '0';
      createApp({ render: () => h(Detail, { key: model.value.id, model: model.value, instanceMode: instanceMode.value, programApi: api }) }).mount(root);
      window.inspectorTest = { ids, definitions, model, instanceMode };
    });
    const info = page.getByRole('textbox', { name: '组件详细信息' });
    await page.locator('.organism-preview svg').waitFor();
    const ids = await page.evaluate(() => window.inspectorTest.ids);
    assert((await info.inputValue()).includes(ids[0]));
    assert((await info.inputValue()).includes('耐久上限: 100'));
    assert(!(await info.inputValue()).includes('当前耐久'));
    const columns = await page.evaluate(() => ['.organism-heading', '.component-information', '.organism-preview'].map(selector => {
      const rect = document.querySelector(selector).getBoundingClientRect();
      return { x: rect.x, right: rect.right, y: rect.y };
    }));
    assert(columns[0].right <= columns[1].x && columns[1].right <= columns[2].x, 'Details must sit between the name and appearance');
    assert(Math.abs(columns[0].y - columns[1].y) < 2 && Math.abs(columns[1].y - columns[2].y) < 2, 'Three columns must share the same row');
    assert(columns[1].right - columns[1].x <= 280, 'Information panel should be at most half its previous desktop width');
    assert.equal(await page.locator('.preview-component').count(), 4);
    await page.locator(`[data-component-id="${ids[1]}"]`).hover();
    assert((await info.inputValue()).includes(ids[1]));
    assert((await info.inputValue()).includes('名称: 反应结构I'));
    assert((await info.inputValue()).includes('非气体容量: 20000 mg'));
    for (const field of ['气体容量:', '能量容量:', '传感器槽位:', '质量:']) assert(!(await info.inputValue()).split('\n').some(line => line.startsWith(field)), field);
    const external = await page.locator(`[data-component-id="${ids[2]}"] path`).first().evaluate(path => {
      const point = path.getPointAtLength(path.getTotalLength() / 2);
      const position = new DOMPoint(point.x, point.y).matrixTransform(path.getScreenCTM());
      return { x: position.x, y: position.y };
    });
    await page.mouse.move(external.x, external.y);
    assert((await info.inputValue()).includes(ids[2]));
    assert((await info.inputValue()).includes('喷射器官I'));
    for (const field of ['气体容量:', '非气体容量:', '能量容量:', '最低反应速率:', '最高反应速率:', '传感器槽位:']) assert(!(await info.inputValue()).split('\n').some(line => line.startsWith(field)), field);
    const infoBounds = await info.boundingBox();
    await page.mouse.move(infoBounds.x + 10, infoBounds.y + 10);
    assert((await info.inputValue()).includes(ids[0]), 'Leaving an unclicked component must restore the selected body');
    await page.mouse.click(external.x, external.y);
    await page.mouse.move(infoBounds.x + 10, infoBounds.y + 10);
    await info.focus();
    assert((await info.inputValue()).includes(ids[2]), 'Clicking must pin the component for copying');
    await page.locator(`[data-component-id="${ids[1]}"]`).hover();
    assert((await info.inputValue()).includes(ids[1]));
    await page.mouse.move(infoBounds.x + 10, infoBounds.y + 10);
    assert((await info.inputValue()).includes(ids[2]), 'Hovering another component must not replace the pinned component');
    await page.locator(`[data-component-id="${ids[3]}"]`).focus();
    assert((await info.inputValue()).includes('皮肤I'));
    await info.focus();
    assert((await info.inputValue()).includes(ids[2]), 'Keyboard preview must also restore the pinned selection');
    await page.locator(`[data-component-id="${ids[3]}"]`).focus();
    await page.keyboard.press('Enter');
    await info.focus();
    assert((await info.inputValue()).includes(ids[3]), 'Enter must pin a focused component');
    await page.waitForFunction(() => document.querySelector('.component-information-text').scrollTop === 0);
    const geometry = await page.evaluate(async () => {
      const svg = document.querySelector('.organism-preview svg');
      const [x, y, width, height] = svg.getAttribute('viewBox').split(' ').map(Number);
      const bounds = svg.querySelector('g').getBBox();
      const assertWithin = bounds.x >= x && bounds.y >= y && bounds.x + bounds.width <= x + width && bounds.y + bounds.height <= y + height;
      const copy = svg.cloneNode(true); copy.setAttribute('width', '320'); copy.setAttribute('height', '280');
      const image = new Image(); image.src = `data:image/svg+xml,${encodeURIComponent(new XMLSerializer().serializeToString(copy))}`;
      await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = 320; canvas.height = 280;
      const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0);
      const pixels = ctx.getImageData(0, 0, 320, 280).data;
      let colored = 0;
      for (let index = 0; index < pixels.length; index += 4) if (pixels[index + 3] > 0) colored++;
      return { within: assertWithin, colored };
    });
    assert(geometry.within, 'All component geometry must fit the preview');
    assert(geometry.colored > 1000, JSON.stringify(geometry));
    const desktopScreenshot = path.join(os.tmpdir(), 'life-detail-inspector-desktop.png');
    await page.screenshot({ path: desktopScreenshot, fullPage: true });
    await page.evaluate(() => {
      const test = window.inspectorTest;
      test.instanceMode.value = true;
      test.model.value = { ...test.model.value, id: 'instance-1', name: '测试生物实例', ai: {}, component_states: Object.fromEntries(test.ids.map((id, i) => [id, {
        properties: test.definitions[i].properties, durability: 74, energy_mj: i === 0 ? 250 : 0,
        container: { non_gas_capacity_mg: test.definitions[i].properties.non_gas_capacity_mg, gas_capacity_ul: test.definitions[i].properties.gas_capacity_ul, non_gas_contents_mg: i === 0 ? { water: 31 } : {}, gas_contents_ul: i === 0 ? { oxygen: 12 } : {} },
      }])) };
    });
    await page.getByText('测试生物实例', { exact: true }).waitFor();
    await page.locator(`[data-component-id="${ids[0]}"]`).focus();
    assert((await info.inputValue()).includes('当前耐久: 74 / 100'));
    assert((await info.inputValue()).includes('water: 31 mg'));
    assert((await info.inputValue()).includes('oxygen: 12 µL'));
    await page.locator(`[data-component-id="${ids[2]}"]`).focus();
    assert((await info.inputValue()).includes('当前耐久: 74 / 100'));
    for (const field of ['当前能量:', '气体容量:', '非气体容量:', '气体内容物:', '非气体内容物:']) assert(!(await info.inputValue()).split('\n').some(line => line.startsWith(field)), field);
    await page.locator(`[data-component-id="${ids[0]}"]`).focus();
    await page.screenshot({ path: path.join(os.tmpdir(), 'life-detail-inspector-instance.png'), fullPage: true });
    for (const width of [1024, 390, 1280]) {
      await page.setViewportSize({ width, height: 900 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `Overflow at ${width}`);
      const bounds = await page.locator('.organism-preview').boundingBox();
      assert(bounds.width > 0 && bounds.height >= 190);
      if (width === 390) await page.screenshot({ path: path.join(os.tmpdir(), 'life-detail-inspector-mobile.png'), fullPage: true });
    }
    await page.evaluate(async () => { const { useDataStore } = await import('/src/stores/data.js'); useDataStore().allComponents.value = []; });
    await page.getByText('暂无外观数据', { exact: true }).waitFor();
    assert((await info.inputValue()).includes('当前耐久: 74 / 100'), 'Snapshot state survives a missing definition');
    assert.deepEqual(errors, []);
    console.log('PASS: model and instance component hover, UUID and properties, persistent inspection, keyboard focus, fitting, rendered pixels and responsive layout.', geometry, desktopScreenshot);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
