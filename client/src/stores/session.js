import { computed, ref } from 'vue'
import { ApiError, authApi, getToken, setToken } from '../api/http'

const token = ref(getToken())
const profile = ref(null)
const loading = ref(false)
const error = ref('')

export function useSession() {
  const authenticated = computed(() => Boolean(token.value && profile.value))

  async function restore() {
    if (!token.value) return false
    loading.value = true; error.value = ''
    try { profile.value = (await authApi.me()).data; return true }
    catch (cause) { if (cause instanceof ApiError && cause.status === 401) { setToken(''); token.value = ''; profile.value = null }; error.value = cause.message; return false }
    finally { loading.value = false }
  }

  async function login(payload) {
    loading.value = true; error.value = ''
    try { const result = await authApi.login(payload); setToken(result.data.token); token.value = result.data.token; profile.value = (await authApi.me()).data; return true }
    catch (cause) { setToken(''); token.value = ''; profile.value = null; error.value = cause.message; return false }
    finally { loading.value = false }
  }

  async function register(payload) {
    loading.value = true; error.value = ''
    try { await authApi.register(payload); return true }
    catch (cause) { error.value = cause.message; return false }
    finally { loading.value = false }
  }

  async function logout() {
    const shouldRevoke = Boolean(token.value)
    if (shouldRevoke) {
      try { await authApi.logout() } catch {}
    }
    clearLocalSession()
  }

  function clearLocalSession() {
    setToken(''); token.value = ''; profile.value = null; error.value = ''
  }

  function clearError() { error.value = '' }

  async function deleteAccount() { await authApi.deleteAccount(); await logout() }
  return { token, profile, loading, error, authenticated, restore, login, register, logout, clearLocalSession, clearError, deleteAccount }
}
