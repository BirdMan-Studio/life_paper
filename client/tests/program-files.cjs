const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ acceptDownloads: true });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1425/');
    await page.evaluate(async () => {
      const source = await (await fetch('/src/views/ModelDetailView.vue')).text();
      const { createApp, h } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Detail } = await import('/src/views/ModelDetailView.vue');
      const { readProgramFile, programFileBlob } = await import('/src/api/programFile.js');
      for (const [filename, bytes] of [ ['data.py', [255, 128]], ['data.txt', [0, 65]], ['data.bin', [65, 66]] ]) {
        const result = await readProgramFile(new Blob([new Uint8Array(bytes)]), filename);
        if (result.text !== null) throw new Error(`${filename} was editable`);
        if (String(new Uint8Array(await result.blob.arrayBuffer())) !== String(bytes)) throw new Error('Binary bytes changed');
      }
      const bytes = new Uint8Array([239, 187, 191, 65, 13, 10]);
      const text = await readProgramFile(new Blob([bytes]), 'test.py');
      if (String(new Uint8Array(await programFileBlob(text.blob, text.text, text.text).arrayBuffer())) !== String(bytes)) throw new Error('BOM/CRLF changed');
      const files = [
        { name: 'child', path: 'child', kind: 'folder' },
        { name: 'weights.bin', path: 'weights.bin', kind: 'file' },
        { name: 'main.py', path: 'main.py', kind: 'file' },
      ];
      const state = window.fileTest = { pending: [], uploads: [], delayed: false };
      const api = {
        folder: async (_id, path) => {
          if (state.delayed) return new Promise((resolve, reject) => state.pending.push({ path, resolve, reject }));
          return { data: files };
        },
        file: async (_id, path) => new Blob([path === 'weights.bin' ? new Uint8Array([255, 128]) : 'print("ok")\r\n']),
        upload: async (_id, path, content) => state.uploads.push({ path, bytes: [...new Uint8Array(await new Blob([content]).arrayBuffer())] }),
      };
      const root = document.createElement('div'); document.body.replaceChildren(root);
      createApp({ render: () => h(Detail, { model: { id: 'model', name: 'Files', composition: {}, updated_at: '2026-11-19' }, programApi: api }) }).mount(root);
    });
    await page.getByRole('button', { name: 'weights.bin', exact: true }).click();
    await page.getByText('二进制文件', { exact: false }).waitFor();
    assert.equal(await page.locator('.code-panel .cm-editor').count(), 0);
    assert.equal(await page.locator('.code-actions button[title="保存"]').count(), 0);
    const downloadPromise = page.waitForEvent('download');
    await page.locator('.code-actions button[title="下载"]').click();
    const download = await downloadPromise;
    assert.deepEqual([...await fs.readFile(await download.path())], [255, 128]);
    await page.getByRole('button', { name: 'main.py', exact: true }).click();
    const editor = page.locator('.code-panel .cm-content');
    await editor.waitFor();
    await page.locator('.code-actions button[title="保存"]').click();
    await page.waitForFunction(() => window.fileTest.uploads.length === 1);
    assert.deepEqual(await page.evaluate(() => window.fileTest.uploads[0].bytes), [...Buffer.from('print("ok")\r\n')]);
    await editor.click(); await page.keyboard.press('Control+End'); await page.keyboard.type('# edited');
    await page.locator('.code-actions button[title="保存"]').click();
    await page.waitForFunction(() => window.fileTest.uploads.length === 2);
    assert(Buffer.from(await page.evaluate(() => window.fileTest.uploads[1].bytes)).toString().includes('# edited'));

    // Resolve the latest root request first, then a stale child response.
    await page.evaluate(() => { window.fileTest.delayed = true; });
    await page.getByRole('button', { name: 'child', exact: true }).click();
    await page.locator('.crumb').first().click();
    await page.waitForFunction(() => window.fileTest.pending.length === 2);
    await page.evaluate(() => window.fileTest.pending[1].resolve({ data: [{ name: 'latest.py', path: 'latest.py', kind: 'file' }] }));
    await page.getByRole('button', { name: 'latest.py', exact: true }).waitFor();
    await page.evaluate(() => window.fileTest.pending[0].resolve({ data: [{ name: 'stale.py', path: 'child/stale.py', kind: 'file' }] }));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.locator('.crumb').count(), 1);
    assert.equal(await page.getByRole('button', { name: 'stale.py', exact: true }).count(), 0);
    await page.locator('input[type=file]').first().setInputFiles({ name: 'new.py', mimeType: 'text/plain', buffer: Buffer.from('new') });
    await page.waitForFunction(() => window.fileTest.uploads.length === 3);
    assert.equal(await page.evaluate(() => window.fileTest.uploads[2].path), 'new.py');
    // An older error must neither clear the latest spinner nor display a toast.
    await page.locator('.refresh').click();
    await page.locator('.refresh').click();
    await page.waitForFunction(() => window.fileTest.pending.length === 5);
    await page.evaluate(() => window.fileTest.pending[3].reject(new Error('stale-folder-error')));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.getByText('正在读取目录…', { exact: true }).count(), 1);
    assert.equal(await page.getByText('stale-folder-error', { exact: true }).count(), 0);
    await page.evaluate(() => window.fileTest.pending[4].resolve({ data: [] }));
    await page.getByText('当前目录为空', { exact: true }).waitFor();
    assert.deepEqual(errors, []);
    console.log('PASS: raw binary download, text editing, byte-preserving saves, and stale directory responses');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
