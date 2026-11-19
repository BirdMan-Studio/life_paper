import { computed, ref } from 'vue'
import { ApiError, authApi, getApiBase, getToken, setToken, getSessionVersion } from '../api/http'

const token = ref(getToken())
const profile = ref(null)
const loading = ref(false)
const error = ref('')

export function useSession() {
  const authenticated = computed(() => Boolean(token.value && profile.value))

  async function restore() {
    if (!token.value) return false
    const version = getSessionVersion()
    loading.value = true; error.value = ''
    try { const result = await authApi.me(); if (version !== getSessionVersion()) return false; profile.value = result.data; return true }
    catch (cause) { if (version !== getSessionVersion()) return false; if (cause instanceof ApiError && cause.status === 401) { setToken(''); token.value = ''; profile.value = null }; error.value = cause.message; return false }
    finally { if (version === getSessionVersion() || !token.value) loading.value = false }
  }

  async function login(payload) {
    const base = getApiBase()
    let version = getSessionVersion()
    const isCurrent = () => base === getApiBase() && version === getSessionVersion()
    loading.value = true; error.value = ''
    try {
      const result = await authApi.login(payload)
      if (!isCurrent()) return false
      setToken(result.data.token)
      version = getSessionVersion()
      token.value = result.data.token
      const account = await authApi.me()
      if (!isCurrent()) return false
      profile.value = account.data
      return true
    }
    catch (cause) { if (!isCurrent()) return false; setToken(''); token.value = ''; profile.value = null; error.value = cause.message; return false }
    finally { if (isCurrent() || !token.value) loading.value = false }
  }

  async function register(payload) {
    loading.value = true; error.value = ''
    try { await authApi.register(payload); return true }
    catch (cause) { error.value = cause.message; return false }
    finally { loading.value = false }
  }

  async function logout() {
    const version = getSessionVersion()
    const shouldRevoke = Boolean(token.value)
    if (shouldRevoke) {
      try { await authApi.logout() } catch {}
    }
    if (version === getSessionVersion()) clearLocalSession()
  }

  function clearLocalSession() {
    setToken(''); token.value = ''; profile.value = null; error.value = ''; loading.value = false
  }

  function clearError() { error.value = '' }

  async function deleteAccount() { const version = getSessionVersion(); await authApi.deleteAccount(); if (version === getSessionVersion()) await logout() }
  return { token, profile, loading, error, authenticated, restore, login, register, logout, clearLocalSession, clearError, deleteAccount }
}
