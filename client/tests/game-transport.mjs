import { test } from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import { resolveRelayBase, selectGameTransport, fetchGameTransport } from '../src/api/gameTransport.js'

const direct = 'https://game.test/api/v1', relay = 'https://central.test/api/v1/relay/game-1'
const transport = () => ({ base: direct, directBase: direct, relayBase: relay, usingRelay: false })
const nativeFetch = globalThis.fetch

test('relay address must belong to the configured center and server', () => {
  assert.equal(resolveRelayBase('https://central.test/api/v1', 'game-1', '/api/v1/relay/game-1'), relay)
  for (const advertised of ['https://evil.test/api/v1/relay/game-1', '//evil.test', '/api/v1/relay/other']) {
    assert.equal(resolveRelayBase('https://central.test/api/v1', 'game-1', advertised), '')
  }
})

test('initial unreachable probe selects relay without sending credentials', async () => {
  globalThis.fetch = async (_url, options) => { assert.equal(options.headers, undefined); throw new TypeError('offline') }
  assert.equal((await selectGameTransport(direct, relay, 'game-1')).base, relay)
})

test('reachable wrong server identity is rejected', async () => {
  globalThis.fetch = async () => Response.json({ data: { server_id: 'other', auth_mode: 'central', protocol_version: 1 } })
  await assert.rejects(selectGameTransport(direct, relay, 'game-1'), /身份/)
})

test('read failure retries on relay; raw upload and subsequent reads retain relay', async () => {
  const calls = [], state = transport()
  globalThis.fetch = async (url, options) => {
    calls.push([url, options])
    if (url.startsWith(direct)) throw new TypeError('offline')
    return new Response('code', { headers: { 'content-type': 'application/octet-stream' } })
  }
  assert.equal(await (await fetchGameTransport(state, '/worlds/1/files?path=main.py')).text(), 'code')
  const file = new Blob(['uploaded'])
  await fetchGameTransport(state, '/worlds/1/files?path=main.py', { method: 'PUT', body: file })
  assert.equal(calls.length, 3)
  assert.equal(calls[2][0], `${relay}/worlds/1/files?path=main.py`)
  assert.equal(calls[2][1].body, file)
})

test('write failure is not replayed and switches following requests to relay', async () => {
  let count = 0
  const state = transport()
  globalThis.fetch = async () => { count++; throw new TypeError('offline') }
  await assert.rejects(fetchGameTransport(state, '/worlds', { method: 'POST', body: '{}' }), /操作结果未知/)
  assert.equal(count, 1)
  assert.equal(state.base, relay)
})

test('authorization rejection does not trigger relay', async () => {
  const state = transport()
  globalThis.fetch = async () => new Response('', { status: 401 })
  assert.equal((await fetchGameTransport(state, '/worlds')).status, 401)
  assert.equal(state.usingRelay, false)
})

test('caller cancellation does not trigger relay or replay', async () => {
  const state = transport(), controller = new AbortController()
  controller.abort()
  globalThis.fetch = async (_url, options) => { throw options.signal.reason }
  await assert.rejects(fetchGameTransport(state, '/worlds', { signal: controller.signal }))
  assert.equal(state.usingRelay, false)
})

test('body interruption after real HTTP headers retries reads but never replays writes', async () => {
  globalThis.fetch = nativeFetch
  const calls = []
  const server = http.createServer((req, res) => {
    calls.push([req.method, req.url])
    if (req.url.startsWith('/direct/')) {
      res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': 1000 })
      res.write('{"data":')
      setTimeout(() => res.destroy(), 25)
    } else res.end('{"data":"relayed"}')
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const origin = `http://127.0.0.1:${server.address().port}`
  const state = () => ({ base: `${origin}/direct`, directBase: `${origin}/direct`, relayBase: `${origin}/relay`, usingRelay: false })
  try {
    const read = state()
    assert.deepEqual(await (await fetchGameTransport(read, '/worlds')).json(), { data: 'relayed' })
    assert.equal(read.usingRelay, true)
    assert.deepEqual(calls, [['GET', '/direct/worlds'], ['GET', '/relay/worlds']])
    calls.length = 0
    const write = state()
    await assert.rejects(fetchGameTransport(write, '/worlds', { method: 'POST', body: '{}' }), /操作结果未知/)
    assert.equal(write.usingRelay, true)
    assert.deepEqual(calls, [['POST', '/direct/worlds']])
    calls.length = 0
    const cancelled = state(), controller = new AbortController()
    const request = fetchGameTransport(cancelled, '/worlds', { signal: controller.signal })
    const abort = setTimeout(() => controller.abort(), 10)
    try { await assert.rejects(request) } finally { clearTimeout(abort) }
    assert.equal(cancelled.usingRelay, false)
    assert(calls.every(([, url]) => url.startsWith('/direct/')))
  } finally {
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
  }
})

test('buffering preserves binary content, HTTP errors and empty response semantics', async () => {
  for (const status of [204, 205, 304]) {
    globalThis.fetch = async () => new Response(null, { status })
    const response = await fetchGameTransport(transport(), '/worlds')
    assert.equal(response.status, status)
    assert.equal(await response.text(), '')
  }
  globalThis.fetch = async () => new Response(new Uint8Array([0, 128, 255]), { headers: { 'content-type': 'application/octet-stream' } })
  const response = await fetchGameTransport(transport(), '/files')
  assert.equal(response.headers.get('content-type'), 'application/octet-stream')
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), new Uint8Array([0, 128, 255]))
  globalThis.fetch = async () => new Response(null)
  assert.equal((await fetchGameTransport(transport(), '/worlds', { method: 'HEAD' })).body, null)
})
