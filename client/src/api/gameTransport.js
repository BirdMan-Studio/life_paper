export function resolveRelayBase(central, id, advertised) {
  const url = new URL(central)
  const expected = `${url.pathname.replace(/\/$/, '')}/relay/${encodeURIComponent(id)}`
  if (advertised !== expected) return ''
  return `${url.origin}${expected}`
}

async function timedFetch(url, options, timeout) {
  const signal = options.signal ? AbortSignal.any([options.signal, AbortSignal.timeout(timeout)]) : AbortSignal.timeout(timeout)
  return fetch(url, { ...options, signal, redirect: 'error' })
}

export async function selectGameTransport(directBase, relayBase, serverId, signal) {
  const transport = { base: directBase, directBase, relayBase, usingRelay: false }
  let response
  try { response = await timedFetch(`${directBase}/game-server`, { signal }, 3000) }
  catch (error) {
    if (signal?.aborted) throw error
    if (!relayBase) throw new Error('无法连接游戏服')
    transport.base = relayBase; transport.usingRelay = true
    return transport
  }
  const metadata = await response.json().catch(() => null)
  if (!response.ok || metadata?.data?.server_id !== serverId || metadata.data.auth_mode !== 'central' || metadata.data.protocol_version !== 1) {
    throw new Error('游戏服身份或协议与中心登记不一致')
  }
  return transport
}

export async function fetchGameTransport(transport, path, options = {}) {
  const safe = ['GET', 'HEAD'].includes((options.method || 'GET').toUpperCase())
  const readResponse = async () => {
    const response = await timedFetch(`${transport.base}${path}`, options, 35000)
    // Fetch resolves at headers; buffer the body here so interrupted downloads also fail over.
    const body = await response.arrayBuffer()
    return new Response(options.method?.toUpperCase() === 'HEAD' || [204, 205, 304].includes(response.status) ? null : body, {
      status: response.status, statusText: response.statusText, headers: response.headers,
    })
  }
  try { return await readResponse() }
  catch (error) {
    if (options.signal?.aborted) throw error
    if (!transport.usingRelay && transport.relayBase) {
      transport.base = transport.relayBase; transport.usingRelay = true
      if (safe) return readResponse()
    }
    throw new Error(safe ? '连接失败或请求超时' : '连接中断，操作结果未知，请刷新后确认')
  }
}
