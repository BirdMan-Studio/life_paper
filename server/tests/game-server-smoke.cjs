const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { mkdtemp, writeFile, rm, readdir } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const { resolve, join } = require('node:path');
const { randomUUID, generateKeyPairSync, sign } = require('node:crypto');
const { createServer } = require('node:http');

const executable = resolve(__dirname, '../../target/debug', process.platform === 'win32' ? 'game-server.exe' : 'game-server');
const keys = generateKeyPairSync('ed25519');
const publicKey = keys.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64url');
function signed(purpose, userId, data) {
  const now = Math.floor(Date.now() / 1000);
  const payload = Buffer.from(JSON.stringify({ version: 1, purpose, audience: 'smoke', user_id: userId, issued_at: now, expires_at: now + 900, data })).toString('base64url');
  return `${payload}.${sign(null, Buffer.from(payload), keys.privateKey).toString('base64url')}`;
}

async function launch(root) {
  const child = spawn(executable, [], { cwd: root, windowsHide: true, env: { ...process.env, LIFE_PAPER_GAME_CONFIG: join(root, 'game.toml'), RUST_LOG: 'info', NO_COLOR: '1' }, stdio: ['ignore', 'pipe', 'pipe'] });
  let logs = '';
  const exited = new Promise(resolve => child.once('exit', resolve));
  child.stdout.on('data', data => { logs += data; });
  child.stderr.on('data', data => { logs += data; });
  child.on('error', error => { logs += error.message; });
  const stop = async () => { if (child.exitCode === null) child.kill(); await exited; };
  try {
    for (let tries = 0; tries < 200; tries++) {
      const clean = logs.replace(/\x1b\[[0-9;]*m/g, '');
      const match = clean.match(/address=127\.0\.0\.1:(\d+)/);
      if (match) return { base: `http://127.0.0.1:${match[1]}/api/v1`, stop };
      if (child.exitCode !== null) throw new Error(`Game server exited: ${logs}`);
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error(`Game server startup timeout: ${logs}`);
  } catch (error) { await stop(); throw error; }
}

(async () => {
  for (const mode of ['local', 'central']) {
    const root = await mkdtemp(join(tmpdir(), 'life-game-smoke-'));
    const localToken = `${randomUUID()}${randomUUID()}`;
    const token = mode === 'local' ? localToken : signed('game-entry', 7, null);
    const python = process.env.LIFE_PAPER_TEST_PYTHON || 'python';
    const registrationKey = `${randomUUID()}${randomUUID()}`;
    let registrations = 0, rejectRegistration = false;
    const central = createServer(async (req,res) => {
      if (rejectRegistration || req.url !== '/api/v1/game-servers/register' || req.headers.authorization !== `Bearer ${registrationKey}`) { res.writeHead(401); res.end(); return; }
      let body = ''; for await (const chunk of req) body += chunk;
      const value = JSON.parse(body);
      assert.equal(value.server_id,'smoke'); assert.equal(value.api_url,'http://127.0.0.1:8081/api/v1');
      registrations++;
      res.setHeader('Content-Type','application/json'); res.end(JSON.stringify({data:{receipt:signed('game-registration',1,null),lease_seconds:90}}));
    });
    await new Promise(resolve => central.listen(0,'127.0.0.1',resolve));
    const centralBase = `http://127.0.0.1:${central.address().port}/api/v1`;
    await writeFile(join(root, 'game.toml'), `server_id = "smoke"\nhost = "127.0.0.1"\nport = 0\ndata_dir = "data"\n${mode === 'central' ? `[registration]\ncentral_api_url = "${centralBase}"\npublic_api_url = "http://127.0.0.1:8081/api/v1"\nverification_key = "${registrationKey}"\n` : ''}[auth]\nmode = "${mode}"\n${mode === 'local' ? `token = "${localToken}"` : `public_key = "${publicKey}"`}\n[ai]\nenabled = true\npython = ${JSON.stringify(python)}\n`);
    let server;
    try {
      if (mode === 'central') {
        rejectRegistration = true;
        await assert.rejects(launch(root), /central rejected game server registration/);
        rejectRegistration = false;
      }
      server = await launch(root);
      assert.equal(registrations,mode === 'central' ? 1 : 0);
      async function request(path, method = 'GET', body, bearer = token) {
        const response = await fetch(`${server.base}${path}`, { method, headers: { Authorization: `Bearer ${bearer}`, 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
        return { status: response.status, data: await response.json().catch(() => null) };
      }
      assert.equal((await request('/game-server')).data.data.auth_mode, mode);
      assert.equal((await request('/worlds', 'GET', undefined, 'invalid')).status, 401);
      assert.equal((await request('/auth/login', 'POST', {})).status, 404);
      const created = await request('/worlds', 'POST', { level_id: 'smoke', name: 'Standalone smoke' });
      assert.equal(created.status, 200, JSON.stringify(created));
      const world = created.data.data.id;
      const modelId = randomUUID(), bodyId = randomUUID();
      const bundle = { id: modelId, name: 'Offline import', composition: {
        id: modelId, program_id: randomUUID(), root_body: bodyId,
        components: { [bodyId]: { id: bodyId, component: 'small_body_1', placement: { rotation_mdeg: 0, size: { width_px: 0, length_px: 0 } } } }, connections: [],
      }, files: { 'main.py': Buffer.from('def main(game):\n    game.memory["smoke"] = True\n    game.log("standalone")\n').toString('base64url'), 'lib/data.bin': Buffer.from([0, 255, 128]).toString('base64url') } };
      const body = { model_id: modelId, initial_contents: { [bodyId]: { non_gas_contents_mg: { water: 100 } } }, ...(mode === 'local' ? { local_model: bundle } : { model_bundle: signed('model-bundle', 7, bundle) }) };
      const instance = await request(`/worlds/${world}/organisms`, 'POST', body);
      assert.equal(instance.status, 201, JSON.stringify(instance));
      const id = instance.data.data.id;
      assert.equal((await request(`/worlds/${world}/resume`, 'POST')).status, 200);
      let ran = false;
      for (let tries = 0; tries < 100; tries++) {
        const state = await request(`/worlds/${world}/organisms/${id}/ai`);
        if (state.data.data.state.memory.smoke) { assert.equal(state.data.data.state.last_run.ok, true); ran = true; break; }
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      assert(ran, 'Standalone Python AI must execute');
      assert.equal((await request(`/worlds/${world}/pause`, 'POST')).status, 200);
      if (mode === 'central') {
        const management = signed('game-admin', 1, null);
        assert.equal((await request('/management/logs')).status, 401);
        const events = await request('/management/logs', 'GET', undefined, management);
        assert.equal(events.status, 200);
        assert(events.data.data.records.some(record => record.category === 'network' && record.fields.path === '/api/v1/worlds'));
        assert(events.data.data.records.some(record => record.category === 'runtime'));
        assert(!JSON.stringify(events).includes(management));
        assert.equal((await request('/management/worlds', 'GET', undefined, management)).data.data.length, 1);
      } else {
        assert.equal((await request('/management/logs')).status, 403);
      }
      await server.stop();
      server = await launch(root);
      const restored = await request(`/worlds/${world}/organisms`);
      assert.equal(restored.data.data.length, 1);
      assert.equal(restored.data.data[0].ai.memory.smoke, true);
      assert.equal((await request('/worlds')).data.data[0].status, 'paused');
      if (mode === 'central') {
        const events = await request('/management/logs?category=network', 'GET', undefined, signed('game-admin', 1, null));
        assert(events.data.data.records.some(record => record.fields.path === `/api/v1/worlds/${world}/pause`), 'Network history must survive process restart');
      }
      assert.deepEqual((await readdir(root)).sort(), ['data', 'game.toml']);
      console.log(`PASS: standalone ${mode} process starts without PostgreSQL/Redis/config.toml, imports models, runs Python AI, pauses and restores saves.`);
    } finally {
      if (server) await server.stop();
      await new Promise(resolve => central.close(resolve));
      await rm(root, { recursive: true, force: true });
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
