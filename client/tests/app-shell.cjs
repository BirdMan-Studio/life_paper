const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/components/AppShell.vue')).text();
      const { createApp, h, ref } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: AppShell } = await import('/src/components/AppShell.vue');
      const compact = ref(false);
      const root = document.createElement('div'); document.body.replaceChildren(root);
      createApp({ render: () => h(AppShell, {
        route: 'world', compact: compact.value, label: 'World',
        profile: { username: 'Player' }, wallet: { energy_coins: 100, gold_coins: 20 },
        onToggle: () => { compact.value = !compact.value; },
      }, { default: () => h('section', { class: 'content', style: { height: '2400px' } }, 'Scrollable content') }) }).mount(root);
    });
    for (const viewport of [{ width: 1440, height: 900 }, { width: 1100, height: 700 }]) {
      await page.setViewportSize(viewport);
      for (const compact of [false, true]) {
        if (compact) { await page.locator('.menu-toggle').click(); await page.waitForTimeout(300); }
        await page.locator('.main-scroll').evaluate(element => { element.scrollTop = 0; });
        const sidebar = await page.locator('.sidebar').boundingBox();
        const topbar = await page.locator('.topbar').boundingBox();
        const footer = await page.locator('.sidebar-footer').boundingBox();
        await page.mouse.move(viewport.width - 100, 400);
        await page.mouse.wheel(0, 650);
        await page.waitForTimeout(200);
        assert(await page.locator('.main-scroll').evaluate(element => element.scrollTop > 0));
        assert.equal(await page.evaluate(() => window.scrollY), 0);
        assert.deepEqual(await page.locator('.sidebar').boundingBox(), sidebar);
        assert.deepEqual(await page.locator('.topbar').boundingBox(), topbar);
        assert.deepEqual(await page.locator('.sidebar-footer').boundingBox(), footer);
        assert.equal(sidebar.height, viewport.height);
        assert(topbar.x >= sidebar.width, 'Topbar must not overlap the sidebar');
      }
      await page.locator('.menu-toggle').click(); await page.waitForTimeout(300);
    }
    await page.screenshot({ path: 'app-shell.png' });
    assert.deepEqual(errors, []);
    console.log('PASS: only content scrolls; sidebar, footer and topbar remain fixed at both desktop sizes, expanded and compact.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
