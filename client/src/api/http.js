import { useI18n } from '../i18n'
import { resolveRelayBase, selectGameTransport, fetchGameTransport } from './gameTransport'
import { localWorlds } from './localWorlds'

const SERVER_KEY = 'life-paper-server'
const TOKEN_KEY = 'life-paper-token'
const gameSessions = new Map()
let combined = false
let worldConnection = null
let localConnection = null
let connectionGeneration = 0
export function leaveWorld() {
  worldConnection?.controller.abort(); worldConnection = null
  localConnection = null
  for (const [key, session] of gameSessions) if (session.pending) gameSessions.delete(key)
}
export function enterWorld(id) { leaveWorld(); worldConnection = { id, controller: new AbortController() } }
export async function enterLocalWorld(id, mode) {
  enterWorld(id)
  const signal = worldConnection.controller.signal
  const connection = await localWorlds.connect(mode, signal)
  signal.throwIfAborted()
  localConnection = connection
}
function resetConnections() { connectionGeneration++; leaveWorld(); gameSessions.clear(); combined = false }
export function getApiBase() { return (localStorage.getItem(SERVER_KEY) || 'http://127.0.0.1:8080/api/v1').replace(/\/$/, '') }
export function setApiBase(value) { localStorage.setItem(SERVER_KEY, value.trim().replace(/\/$/, '')); resetConnections() }
export function getToken() { return localStorage.getItem(TOKEN_KEY) || '' }
export function setToken(value) { value ? localStorage.setItem(TOKEN_KEY, value) : localStorage.removeItem(TOKEN_KEY); resetConnections() }
export class ApiError extends Error { constructor(message, status = 0) { super(message); this.name = 'ApiError'; this.status = status } }

async function getGameSession(worldId, serverId) {
  const central = getApiBase(), loginToken = getToken()
  const key = JSON.stringify([worldId || '', serverId || '', central, loginToken])
  const cached = gameSessions.get(key)
  if (cached && (cached.pending || cached.expiresAt > Date.now() + 30000)) return cached.pending || cached
  const entry = { key }
  const generation = connectionGeneration
  const signal = worldId ? worldConnection?.controller.signal : undefined
  entry.pending = (async () => {
    const result = await apiRequest(worldId ? `/game-worlds/${encodeURIComponent(worldId)}/connection` : `/game-servers/${encodeURIComponent(serverId)}/connection`, { signal })
    if (generation !== connectionGeneration) throw new ApiError('登录状态已变更')
    signal?.throwIfAborted()
    const id = result.data.game_server_id
    const transport = await selectGameTransport(result.data.direct_url || result.data.api_url, resolveRelayBase(central, id, result.data.relay_url), id, signal)
    signal?.throwIfAborted()
    if (generation !== connectionGeneration) throw new ApiError('登录状态已变更')
    return { key, transport, serverId: id, token: result.data.token, expiresAt: Date.now() + result.data.expires_in * 1000 }
  })()
  gameSessions.set(key, entry)
  try {
    const result = await entry.pending
    if (gameSessions.get(key) === entry) gameSessions.set(key, result)
    return result
  } catch (error) { if (gameSessions.get(key) === entry) gameSessions.delete(key); throw error }
}

async function requestTarget(path, options) {
  if (localConnection && (path === '/python-api' || /^\/worlds(?:\/|$|\?)/.test(path))) {
    const worldId = options?.gameWorldId || /^\/worlds\/([^/?]+)/.exec(path)?.[1]
    if (path !== '/python-api' && worldConnection?.id !== worldId) throw new ApiError('请先进入该世界')
    return { base: localConnection.api_url, token: localConnection.token, local: true, signal: worldConnection.controller.signal }
  }
  if (!combined && /^\/worlds(?:\/|$|\?)/.test(path)) {
    const worldId = options?.gameWorldId || /^\/worlds\/([^/?]+)/.exec(path)?.[1]
    if (options?.gameServerId) return { ...await getGameSession(null, options.gameServerId), game: true }
    if (!worldId || worldId === 'tutorial') throw new ApiError('请先选择世界服务器')
    if (worldConnection?.id !== worldId) throw new ApiError('请先进入该世界')
    const signal = worldConnection.controller.signal
    const session = await getGameSession(worldId)
    signal.throwIfAborted()
    return { ...session, signal, game: true }
  }
  return { base: getApiBase(), token: getToken(), game: false }
}

function handleUnauthorized(response, target) {
  if (response.status !== 401) return
  if (target.local) throw new ApiError('本地游戏引擎凭证已失效，请重新进入世界', 401)
  else if (target.game) gameSessions.delete(target.key)
  else window.dispatchEvent(new CustomEvent('life-paper:unauthorized'))
}

function checkWorldEndpoint(response, path, target) {
  if (response.status === 404 && !target.game && /^\/worlds(?:\?|$)/.test(path)) {
    throw new ApiError('当前服务器未提供世界接口，请检查鉴权服务的部署模式和版本。', 404)
  }
}

export async function apiRequest(path, options = {}) {
  const target = await requestTarget(path, options)
  if (target.signal) options = { ...options, signal: options.signal ? AbortSignal.any([options.signal, target.signal]) : target.signal }
  const { t } = useI18n(); const headers = new Headers(options.headers || {}); const token = target.token; if (token) headers.set('Authorization', `Bearer ${token}`); if (options.body && !(options.body instanceof Blob) && !(options.body instanceof ArrayBuffer)) headers.set('Content-Type', 'application/json')
  let response; try { response = target.game ? await fetchGameTransport(target.transport, path, { ...options, headers }) : await fetch(`${target.base}${path}`, { ...options, headers, redirect: 'error' }) } catch (error) { throw new ApiError(target.game ? error.message : t('errors.connection')) }
  const body = await response.json().catch(() => null); handleUnauthorized(response, target); checkWorldEndpoint(response, path, target); if (!response.ok) throw new ApiError(body?.error?.message || t('errors.request', { status: response.status }), response.status); return response.status === 204 ? null : body
}
export async function apiRequestRaw(path, options = {}) {
  const target = await requestTarget(path, options)
  if (target.signal) options = { ...options, signal: options.signal ? AbortSignal.any([options.signal, target.signal]) : target.signal }
  const { t } = useI18n(); const headers = new Headers(options.headers || {}); const token = target.token; if (token) headers.set('Authorization', `Bearer ${token}`); let response; try { response = target.game ? await fetchGameTransport(target.transport, path, { ...options, headers }) : await fetch(`${target.base}${path}`, { ...options, headers, redirect: 'error' }) } catch (error) { throw new ApiError(target.game ? error.message : t('errors.connection')) }
  handleUnauthorized(response, target)
  if (!response.ok) { const body = await response.json().catch(() => null); throw new ApiError(body?.error?.message || t('errors.request', { status: response.status }), response.status) } return response
}
export const authApi = { register: (payload) => apiRequest('/auth/register', { method: 'POST', body: JSON.stringify(payload) }), login: (payload) => apiRequest('/auth/login', { method: 'POST', body: JSON.stringify(payload) }), me: () => apiRequest('/account/me'), logout: () => apiRequest('/auth/logout', { method: 'POST' }), deleteAccount: () => apiRequest('/account', { method: 'DELETE' }) }
async function listWorlds() {
  const generation = connectionGeneration
  const directory = await apiRequest('/game-servers')
  if (generation !== connectionGeneration) throw new ApiError('登录状态已变更')
  combined = directory.deployment_mode === 'combined'
  if (combined) return { ...await apiRequest('/worlds'), servers: null }
  return { servers: directory.data, data: directory.data.flatMap(server => (server.worlds || []).map(world => ({ ...world, game_server_id: server.id, server_name: server.name }))) }
}
async function createWorld(path, serverId, payload) {
  const generation = connectionGeneration
  if (!combined && !serverId) throw new ApiError('请先选择世界服务器')
  const session = !combined ? await getGameSession(null, serverId) : null
  const result = await apiRequest(path, { method: 'POST', ...(payload ? { body: JSON.stringify(payload) } : {}), gameServerId: serverId })
  if (generation !== connectionGeneration) throw new ApiError('登录状态已变更')
  if (!combined) {
    const worldKey = JSON.stringify([result.data.id, '', getApiBase(), getToken()])
    gameSessions.set(worldKey, { ...session, key: worldKey })
    result.data.game_server_id = serverId
  }
  return result
}
export const worldApi = {
  list: listWorlds,
  details: async (id) => {
    const result = await apiRequest('/worlds', { gameWorldId: id })
    const world = result.data.find(item => item.id === id)
    if (!world) throw new ApiError('世界不存在或当前账号无权访问', 404)
    return { data: world }
  },
  ground: (id) => apiRequest(`/worlds/${id}/ground`), create: (payload, serverId) => createWorld('/worlds', serverId, payload), pause: (id) => apiRequest(`/worlds/${id}/pause`, { method: 'POST' }), resume: (id) => apiRequest(`/worlds/${id}/resume`, { method: 'POST' }),
}
export async function createLocalWorld(mode, name, tutorialPackage, signal) {
  const connection = await localWorlds.connect(mode, signal)
  const path = mode === 'tutorial' ? '/worlds/tutorial/import' : '/worlds'
  const response = await fetch(`${connection.api_url}${path}`, {
    method: 'POST', redirect: 'error', signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(30000)]) : AbortSignal.timeout(30000),
    headers: { Authorization: `Bearer ${connection.token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(mode === 'tutorial' ? tutorialPackage : { name, level_id: 'basic' }),
  })
  const result = await response.json()
  if (!response.ok) throw new ApiError(result?.error?.message || '本地世界创建失败', response.status)
  return result
}
export const gameDirectoryApi = { servers: () => apiRequest('/game-servers'), worldConnection: id => apiRequest(`/game-worlds/${id}/connection`) }
export const playerApi = { data: () => apiRequest('/player/data') }
export const organismApi = {
  list: (worldId) => apiRequest(`/worlds/${worldId}/organisms`),
  create: async (worldId, modelId, initialContents = {}, localModel) => {
    const body = { model_id: modelId, ...(Object.keys(initialContents).length ? { initial_contents: initialContents } : {}) }
    if (localConnection) {
      if (!localModel) throw new ApiError('本地世界暂不支持导入中心模型')
      body.local_model = localModel
    } else if (!combined) {
      const session = await requestTarget(`/worlds/${worldId}/organisms`)
      const exported = await apiRequest(`/organism-models/${modelId}/export`, { method: 'POST', signal: session.signal, body: JSON.stringify({ game_server_id: session.serverId }) })
      body.model_bundle = exported.data.model_bundle
    }
    return apiRequest(`/worlds/${worldId}/organisms`, { method: 'POST', body: JSON.stringify(body) })
  },
  program: (worldId, organismId) => {
    const base = `/worlds/${worldId}/organisms/${organismId}`
    return {
      status: () => apiRequest(`${base}/ai`),
      folder: (_id, path = '') => apiRequest(`${base}/folder?${new URLSearchParams({ path })}`),
      file: async (_id, path) => (await apiRequestRaw(`${base}/files?${new URLSearchParams({ path })}`)).text(),
      upload: (_id, path, file) => apiRequestRaw(`${base}/files?${new URLSearchParams({ path })}`, { method: 'PUT', body: file }),
      createFolder: (_id, path) => apiRequest(`${base}/folders`, { method: 'POST', body: JSON.stringify({ path }) }),
      deleteEntry: (_id, path) => apiRequest(`${base}/entries?${new URLSearchParams({ path })}`, { method: 'DELETE' }),
      renameEntry: (_id, path, name) => apiRequest(`${base}/entries`, { method: 'PATCH', body: JSON.stringify({ path, name }) }),
    }
  },
}
export const modelApi = { list: () => apiRequest('/organism-models'), create: (payload) => apiRequest('/organism-models', { method: 'POST', body: JSON.stringify(payload) }), remove: (id) => apiRequest(`/organism-models/${id}`, { method: 'DELETE' }), folder: (id, path = '') => apiRequest(`/organism-models/${id}/folder?${new URLSearchParams({ path })}`), file: async (id, path) => (await apiRequestRaw(`/organism-models/${id}/files?${new URLSearchParams({ path })}`)).text(), upload: (id, path, file) => apiRequestRaw(`/organism-models/${id}/files?${new URLSearchParams({ path })}`, { method: 'PUT', body: file }), createFolder: (id, path) => apiRequest(`/organism-models/${id}/folders`, { method: 'POST', body: JSON.stringify({ path }) }), deleteEntry: (id, path) => apiRequest(`/organism-models/${id}/entries?${new URLSearchParams({ path })}`, { method: 'DELETE' }), renameEntry: (id, path, name) => apiRequest(`/organism-models/${id}/entries`, { method: 'PATCH', body: JSON.stringify({ path, name }) }) }
export const componentApi = { all: () => apiRequest('/organism-components'), unlocked: () => apiRequest('/organism-components/unlocked'), locked: () => apiRequest('/organism-components/locked'), unlock: (id) => apiRequest(`/organism-components/${id}/unlock`, { method: 'POST' }) }
export const storageKeys = { SERVER_KEY, TOKEN_KEY }
export const pythonApi = { documentation: () => apiRequest('/python-api') }
export const worldMatterApi = {
  sync: (worldId, { x, y, radius_px = 5, since } = {}) => {
    const query = new URLSearchParams({ x, y, radius_px })
    if (since !== undefined && since !== null) query.set('since', since)
    return apiRequest(`/worlds/${worldId}/matter?${query}`)
  },
  history: (worldId, after = 0, limit = 100) => apiRequest(`/worlds/${worldId}/matter/history?${new URLSearchParams({ after, limit })}`),
}
