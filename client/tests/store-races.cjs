const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage();
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    const result = await page.evaluate(async () => {
      const { useDataStore } = await import('/src/stores/data.js');
      // Import through the store's Vite-resolved URL so mocks replace its actual dependencies.
      const source = await (await fetch('/src/stores/data.js')).text();
      const { componentApi, modelApi, playerApi, worldApi } = await import(source.match(/from\s+["']([^"']*\/api\/http[^"']*)["']/)[1]);
      const store = useDataStore();
      const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
      const pending = [];
      worldApi.list = async () => ({ data: [{ id: 'A', status: 'active' }, { id: 'B', status: 'active' }] });
      playerApi.data = async () => ({ data: { energy_coins: 900 } });
      modelApi.list = async () => ({ data: [{ id: 'private-model' }] });
      componentApi.unlocked = componentApi.all = async () => ({ data: [{ id: 'private-component' }] });
      worldApi.ground = id => { const request = { id, ...deferred() }; pending.push(request); return request.promise; };
      store.reset();
      const load = store.load();
      while (!pending.length) await new Promise(resolve => setTimeout(resolve, 0));
      store.reset();
      pending.shift().resolve({ data: { privateGround: true } });
      await load;
      const resetClean = !store.wallet.value && !store.ground.value && !window.lifePaperGround && !store.models.value.length && !store.components.value.length && !store.allComponents.value.length && !store.worlds.value.length;
      store.worlds.value = [{ id: 'A', status: 'active' }, { id: 'B', status: 'active' }];
      store.activeWorldId.value = 'A';
      const selectionB = store.selectWorld('B');
      let paused;
      worldApi.pause = async id => { paused = id; return { data: { id, status: 'paused' } }; };
      await store.setActiveWorldStatus();
      const selectionA = store.selectWorld('A');
      pending[1].resolve({ data: { id: 'A-ground' } });
      await selectionA;
      pending[0].resolve({ data: { id: 'B-ground' } });
      await selectionB;
      const latestWins = store.activeWorldId.value === 'A' && store.ground.value.id === 'A-ground' && window.lifePaperGround.id === 'A-ground';
      pending.length = 0;
      const staleFailure = store.selectWorld('B');
      const latestSelection = store.selectWorld('A');
      pending[1].resolve({ data: { id: 'A-new' } });
      await latestSelection;
      pending[0].reject(new Error('obsolete request'));
      await staleFailure;
      const failureIgnored = store.ground.value.id === 'A-new' && store.error.value === '';
      pending.length = 0;
      const invalidated = store.selectWorld('B');
      store.reset();
      pending[0].reject(new Error('old account'));
      await invalidated;
      return { resetClean, paused, latestWins, failureIgnored, resetFailureIgnored: store.error.value === '' && !store.ground.value && !window.lifePaperGround };
    });
    assert.deepEqual(result, { resetClean: true, paused: 'B', latestWins: true, failureIgnored: true, resetFailureIgnored: true });
    console.log('PASS: logout invalidates async writes; immediate world selection; stale ground success/errors discarded.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
