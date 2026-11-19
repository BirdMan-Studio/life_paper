import { test } from 'node:test'
import assert from 'node:assert/strict'
import { resolveRelayBase, selectGameTransport, fetchGameTransport } from '../src/api/gameTransport.js'

const direct = 'https://game.test/api/v1', relay = 'https://central.test/api/v1/relay/game-1'
const transport = () => ({ base: direct, directBase: direct, relayBase: relay, usingRelay: false })

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
