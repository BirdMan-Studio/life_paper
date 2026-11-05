const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const source = fs.readFileSync(path.join(__dirname, '../../server/src/api/python_docs.rs'), 'utf8');
  const body = source.slice(source.indexOf('"version":'), source.indexOf('"components": components')).trim().replace(/,$/, '');
  const document = { ...JSON.parse(`{${body}}`),
    substances: [{ id: 'water', name: '水', symbol: 'H2O', initial_processing_speed: 1, chemistry: { chemical_energy_mj_per_mg: -32 } }],
    components: [{ id: 'small_body_1', name: '小型躯体I', properties: { max_durability: 100, energy_capacity_mj: 1000, non_gas_capacity_mg: 2000, gas_capacity_ul: 20000 } }],
  };
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    let fail = true;
    await page.route('**/api/v1/python-api', route => fail
      ? route.fulfill({ status: 500, json: { error: { message: '测试连接失败' } } })
      : route.fulfill({ json: { data: document } }));
    await page.goto(process.env.DOCS_TEST_URL || 'http://127.0.0.1:1423/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/views/ApiDocsView.vue')).text();
      const { createApp } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: ApiDocs } = await import('/src/views/ApiDocsView.vue');
      const { setApiBase } = await import('/src/api/http.js');
      setApiBase(`${location.origin}/api/v1`);
      const root = document.createElement('div'); document.body.replaceChildren(root);
      document.body.style.minWidth = '0';
      createApp(ApiDocs).mount(root);
    });
    await page.getByRole('alert').filter({ hasText: '测试连接失败' }).waitFor();
    fail = false;
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await page.locator('.docs-index').waitFor();
    await page.getByRole('button', { name: 'part.jet(material, rate, angle=0)', exact: true }).click();
    assert((await page.locator('.docs-article').innerText()).includes('库存不足'));
    await page.screenshot({ path: path.join(__dirname, 'api-docs-desktop.png'), fullPage: true });
    const search = page.getByRole('searchbox', { name: '搜索文档' });
    await search.fill('absorb_air');
    assert.equal(await page.locator('.docs-index button').count(), 1);
    await search.fill('no-such-method');
    await page.getByText('没有匹配的条目').waitFor();
    await search.fill('');
    await page.getByRole('tab', { name: '物质', exact: true }).click();
    assert((await page.locator('table').innerText()).includes('-32'));
    await page.getByRole('tab', { name: '组件', exact: true }).click();
    assert((await page.locator('table').innerText()).includes('2000'));
    await page.getByRole('tab', { name: 'Python 接口' }).click();
    await page.getByRole('button', { name: 'part.jet(material, rate, angle=0)', exact: true }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: path.join(__dirname, 'api-docs-mobile.png'), fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Mobile page overflow');
    assert.deepEqual(errors, []);
    console.log('API documentation: retry, navigation, search, catalogs, desktop/mobile passed');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
