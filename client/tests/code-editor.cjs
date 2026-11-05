const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => { errors.push(error.message); console.error(error.message); });
    await page.goto('http://127.0.0.1:1423/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/components/CodeEditor.vue')).text();
      const { createApp, ref, h } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: CodeEditor } = await import('/src/components/CodeEditor.vue');
      const root = document.createElement('div'); document.body.replaceChildren(root);
      document.body.style.cssText = 'margin:0;min-width:0;background:#0a111b';
      window.editorTest = { text: ref('def main(game):\n    # 喷射控制\n    part.jet("water", 100, angle=3)\n    return True\n'), filename: ref('main.py'), saves: 0 };
      const state = window.editorTest;
      createApp({ setup: () => () => h(CodeEditor, { modelValue: state.text.value, filename: state.filename.value, 'onUpdate:modelValue': value => { state.text.value = value; }, onSave: () => state.saves++ }) }).mount(root);
    });
    const input = page.locator('.cm-content[contenteditable="true"]');
    await input.waitFor({ timeout: 5000 });
    const colors = await page.locator('.cm-line').evaluateAll(lines => {
      const color = text => {
        const span = lines.flatMap(line => [...line.querySelectorAll('span')]).find(span => span.textContent === text);
        return span && getComputedStyle(span).color;
      };
      return ['def', '"water"', '100', '# 喷射控制'].map(color);
    });
    assert(colors.every(Boolean)); assert.equal(new Set(colors).size, 4);
    await input.click(); await page.keyboard.press('Control+End');
    await page.keyboard.type('# edit');
    assert((await page.evaluate(() => window.editorTest.text.value)).endsWith('# edit'));
    await page.keyboard.press('Control+z');
    assert(!(await page.evaluate(() => window.editorTest.text.value)).endsWith('# edit'));
    await page.keyboard.press('Control+s');
    assert.equal(await page.evaluate(() => window.editorTest.saves), 1);
    await page.evaluate(() => { window.editorTest.text.value = 'print("updated")'; });
    await page.waitForFunction(() => document.querySelector('.cm-content').textContent.includes('updated'));
    await page.evaluate(() => { window.editorTest.filename.value = 'notes.txt'; });
    await page.waitForFunction(() => ![...document.querySelectorAll('.cm-content span')].some(span => getComputedStyle(span).color === 'rgb(168, 217, 155)'));
    await page.evaluate(() => { window.editorTest.filename.value = 'main.py'; });
    await page.waitForFunction(() => [...document.querySelectorAll('.cm-content span')].some(span => getComputedStyle(span).color === 'rgb(168, 217, 155)'));
    await page.screenshot({ path: path.join(__dirname, 'code-editor-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: path.join(__dirname, 'code-editor-mobile.png') });
    assert.deepEqual(errors, []);
    console.log('Python colors, editing, undo, save, external updates, language switching and responsive layout passed');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
