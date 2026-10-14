import { useI18n } from '../i18n'

const SERVER_KEY = 'life-paper-server'
const TOKEN_KEY = 'life-paper-token'
export function getApiBase() { return (localStorage.getItem(SERVER_KEY) || 'http://127.0.0.1:8080/api/v1').replace(/\/$/, '') }
export function setApiBase(value) { localStorage.setItem(SERVER_KEY, value.trim().replace(/\/$/, '')) }
export function getToken() { return localStorage.getItem(TOKEN_KEY) || '' }
export function setToken(value) { value ? localStorage.setItem(TOKEN_KEY, value) : localStorage.removeItem(TOKEN_KEY) }
export class ApiError extends Error { constructor(message, status = 0) { super(message); this.name = 'ApiError'; this.status = status } }

export async function apiRequest(path, options = {}) {
  const { t } = useI18n(); const headers = new Headers(options.headers || {}); const token = getToken(); if (token) headers.set('Authorization', `Bearer ${token}`); if (options.body && !(options.body instanceof Blob) && !(options.body instanceof ArrayBuffer)) headers.set('Content-Type', 'application/json')
  let response; try { response = await fetch(`${getApiBase()}${path}`, { ...options, headers }) } catch { throw new ApiError(t('errors.connection')) }
  const body = await response.json().catch(() => null); if (response.status === 401) window.dispatchEvent(new CustomEvent('life-paper:unauthorized')); if (!response.ok) throw new ApiError(body?.error?.message || t('errors.request', { status: response.status }), response.status); return response.status === 204 ? null : body
}
export async function apiRequestRaw(path, options = {}) {
  const { t } = useI18n(); const headers = new Headers(options.headers || {}); const token = getToken(); if (token) headers.set('Authorization', `Bearer ${token}`); let response; try { response = await fetch(`${getApiBase()}${path}`, { ...options, headers }) } catch { throw new ApiError(t('errors.connection')) }
  if (!response.ok) { const body = await response.json().catch(() => null); if (response.status === 401) window.dispatchEvent(new CustomEvent('life-paper:unauthorized')); throw new ApiError(body?.error?.message || t('errors.request', { status: response.status }), response.status) } return response
}
export const authApi = { register: (payload) => apiRequest('/auth/register', { method: 'POST', body: JSON.stringify(payload) }), login: (payload) => apiRequest('/auth/login', { method: 'POST', body: JSON.stringify(payload) }), me: () => apiRequest('/account/me'), logout: () => apiRequest('/auth/logout', { method: 'POST' }), deleteAccount: () => apiRequest('/account', { method: 'DELETE' }) }
export const worldApi = { list: () => apiRequest('/worlds'), tutorial: () => apiRequest('/worlds/tutorial', { method: 'POST' }), create: (payload) => apiRequest('/worlds', { method: 'POST', body: JSON.stringify(payload) }), pause: (id) => apiRequest(`/worlds/${id}/pause`, { method: 'POST' }), resume: (id) => apiRequest(`/worlds/${id}/resume`, { method: 'POST' }) }
export const playerApi = { data: () => apiRequest('/player/data') }
export const modelApi = { list: () => apiRequest('/organism-models'), create: (payload) => apiRequest('/organism-models', { method: 'POST', body: JSON.stringify(payload) }), remove: (id) => apiRequest(`/organism-models/${id}`, { method: 'DELETE' }), folder: (id, path = '') => apiRequest(`/organism-models/${id}/folder?${new URLSearchParams({ path })}`), file: async (id, path) => (await apiRequestRaw(`/organism-models/${id}/files?${new URLSearchParams({ path })}`)).text(), upload: (id, path, file) => apiRequestRaw(`/organism-models/${id}/files?${new URLSearchParams({ path })}`, { method: 'PUT', body: file }), createFolder: (id, path) => apiRequest(`/organism-models/${id}/folders`, { method: 'POST', body: JSON.stringify({ path }) }), deleteEntry: (id, path) => apiRequest(`/organism-models/${id}/entries?${new URLSearchParams({ path })}`, { method: 'DELETE' }), renameEntry: (id, path, name) => apiRequest(`/organism-models/${id}/entries`, { method: 'PATCH', body: JSON.stringify({ path, name }) }) }
export const componentApi = { all: () => apiRequest('/organism-components'), unlocked: () => apiRequest('/organism-components/unlocked'), locked: () => apiRequest('/organism-components/locked'), unlock: (id) => apiRequest(`/organism-components/${id}/unlock`, { method: 'POST' }) }
export const storageKeys = { SERVER_KEY, TOKEN_KEY }
