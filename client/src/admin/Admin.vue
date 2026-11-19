<script setup>
import { ref, computed, onUnmounted, watch } from 'vue'
import { resolveRelayBase, selectGameTransport, fetchGameTransport } from '../api/gameTransport'
import { Users, Shapes, Server, ScrollText, BookOpen, Globe, Plus, Search, RefreshCw, Pencil, Trash2, X, LogOut, ChevronLeft, ChevronRight, Download, Play, Pause, Eye, ShieldCheck, CircleAlert, KeyRound, Copy } from 'lucide-vue-next'

const base = ref(sessionStorage.getItem('life-admin-base') || `${location.origin}/api/v1`)
const token = ref(sessionStorage.getItem('life-admin-token') || '')
const connected = ref(false), busy = ref(false), saving = ref(false), error = ref(''), notice = ref('')
const view = ref('users'), records = ref([]), total = ref(0), offset = ref(0), search = ref('')
const fleet = ref([]), catalog = ref({ components: [], elements: [] }), target = ref('')
const level = ref(''), category = ref(''), autoRefresh = ref(false), nextBefore = ref(null), logBefore = ref(null)
const modal = ref(null), draft = ref({}), details = ref(null), deleteRow = ref(null)
const issuedKey = ref(null), resetKeyRow = ref(null)
let revision = 0, timer
const nav = [{ id: 'users', name: '用户与资产', icon: Users }, { id: 'models', name: '生物模型', icon: Shapes }, { id: 'servers', name: '游戏服', icon: Server }, { id: 'worlds', name: '世界管理', icon: Globe }, { id: 'logs', name: '日志', icon: ScrollText }, { id: 'catalog', name: '物质与组件', icon: BookOpen }]
const title = computed(() => nav.find(v => v.id === view.value)?.name)
const isLog = computed(() => view.value === 'logs')
const tickets = new Map()
async function fetchJson(url, bearer, options = {}) {
  let response
  try { response = await fetch(url, { ...options, redirect: 'error', headers: { Authorization: `Bearer ${bearer}`, ...(options.body ? { 'Content-Type': 'application/json' } : {}) }, signal: AbortSignal.timeout(15000) }) }
  catch { throw new Error('连接失败或请求超时') }
  if (response.status === 204) return
  const body = await response.json().catch(() => null)
  if (!response.ok) {
    if (response.status === 401 && url.startsWith(`${base.value}/admin/`)) {
      if (connected.value) logout()
      throw new Error('管理员令牌无效，请使用当前鉴权服务的管理员令牌（不是玩家登录令牌）')
    }
    throw new Error(body?.error?.message || `请求失败 (${response.status})`)
  }
  if (body?.data == null) throw new Error(`接口返回格式错误：${new URL(url).pathname}，请检查中心 API 地址及服务器版本`)
  return body.data
}
const request = (path, options) => fetchJson(`${base.value}${path}`, token.value, options)
async function remote(path, options) {
  const id = target.value
  if (!id) throw new Error('请选择游戏服')
  let ticket = tickets.get(id)
  if (!ticket || ticket.expiresAt < Date.now() + 30000) {
    ticket = await request(`/admin/servers/${encodeURIComponent(id)}/ticket`, { method: 'POST' })
    ticket.expiresAt = Date.now() + ticket.expires_in * 1000
    ticket.transport = await selectGameTransport(ticket.api_url, resolveRelayBase(base.value, id, ticket.relay_url), id)
    tickets.set(id, ticket)
  }
  const response = await fetchGameTransport(ticket.transport, `/management${path}`, { ...options, headers: { Authorization: `Bearer ${ticket.token}`, ...(options?.body ? { 'Content-Type': 'application/json' } : {}) } })
  if (response.status === 204) return
  const body = await response.json().catch(() => null)
  if (!response.ok) {
    if (response.status === 401) tickets.delete(id)
    throw new Error(body?.error?.message || `请求失败 (${response.status})`)
  }
  if (body?.data == null) throw new Error('游戏服返回格式错误，请检查游戏服地址及服务器版本')
  return body.data
}
async function login() {
  error.value = ''; busy.value = true
  try {
    token.value = token.value.trim()
    if (!token.value) throw new Error('请输入管理员令牌')
    const url = new URL(base.value.trim().replace(/\/$/, ''))
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error('中心地址无效')
    base.value = url.href.replace(/\/$/, '')
    await request('/admin/session')
    sessionStorage.setItem('life-admin-base', base.value); sessionStorage.setItem('life-admin-token', token.value)
    const [servers, definitions] = await Promise.all([request('/admin/servers'), request('/admin/catalog')])
    fleet.value = servers; catalog.value = definitions; connected.value = true
    await load()
  } catch (e) { error.value = e.message } finally { busy.value = false }
}
function logout() { revision++; connected.value = false; token.value = ''; sessionStorage.removeItem('life-admin-token'); tickets.clear(); records.value = []; modal.value = null; details.value = null; deleteRow.value = null; issuedKey.value = null; resetKeyRow.value = null; autoRefresh.value = false }
async function load() {
  if (!connected.value) return
  const current = ++revision
  busy.value = true; error.value = ''
  try {
    let result
    if (view.value === 'users' || view.value === 'models') {
      result = await request(`/admin/records/${view.value}?${new URLSearchParams({ search: search.value, offset: offset.value, limit: 50 })}`)
      if (current !== revision) return
      if (!Array.isArray(result?.records)) throw new Error('列表响应缺少 records 数组，请检查中心 API 地址及服务器版本')
      records.value = result.records; total.value = result.total
    } else if (view.value === 'servers') {
      result = await request('/admin/servers')
      if (current !== revision) return
      fleet.value = result; records.value = result.filter(r => `${r.id} ${r.name} ${r.api_url}`.toLowerCase().includes(search.value.toLowerCase())); total.value = records.value.length
    } else if (view.value === 'worlds') {
      result = target.value ? await remote('/worlds') : []
      if (current !== revision) return
      records.value = result.filter(r => `${r.id} ${r.name} ${r.owner_user_id}`.toLowerCase().includes(search.value.toLowerCase())); total.value = records.value.length
    } else if (view.value === 'logs') {
      const params = new URLSearchParams({ search: search.value, level: level.value, category: category.value, limit: 100 })
      if (logBefore.value) params.set('before', logBefore.value)
      result = target.value ? await remote(`/logs?${params}`) : await request(`/admin/logs?${params}`)
      if (current !== revision) return
      if (!Array.isArray(result?.records)) throw new Error('日志响应缺少 records 数组，请检查服务地址及服务器版本')
      records.value = result.records; nextBefore.value = result.next_before; total.value = records.value.length
    } else {
      records.value = []; total.value = catalog.value.components.length + catalog.value.elements.length
    }
  } catch (e) { if (current === revision) { error.value = e.message; records.value = [] } }
  finally { if (current === revision) busy.value = false }
}
function resetLoad() { offset.value = 0; logBefore.value = null; load() }
function switchView(id) { records.value = []; view.value = id; search.value = ''; offset.value = 0; logBefore.value = null; notice.value = ''; if (id === 'worlds' && !target.value) target.value = fleet.value[0]?.id || ''; load() }
watch(autoRefresh, value => { clearInterval(timer); if (value) timer = setInterval(() => { if (!busy.value && isLog.value && !logBefore.value) load() }, 5000) })
onUnmounted(() => clearInterval(timer))
function newComposition() {
  const id = crypto.randomUUID(), body = crypto.randomUUID()
  return { id, program_id: id, root_body: body, components: { [body]: { id: body, component: 'small_body_1', placement: {} } }, connections: [] }
}
const copy = value => JSON.parse(JSON.stringify(value))
function edit(row = null) {
  notice.value = ''; error.value = ''; modal.value = { kind: view.value, id: row?.id || null }
  if (view.value === 'users') draft.value = row ? { ...copy(row), password: '', soup: Object.entries(row.bio_soup).map(([element, amount]) => ({ element, amount })) } : { username: '', email: '', password: '', energy_coins: '0', gold_coins: '0', soup: [], unlocked_components: [] }
  else if (view.value === 'models') draft.value = row ? { ...copy(row), composition: JSON.stringify(row.composition, null, 2) } : { owner_user_id: '', name: '', composition: JSON.stringify(newComposition(), null, 2) }
  else if (view.value === 'servers') draft.value = row ? copy(row) : { id: '', name: '', api_url: 'http://127.0.0.1:8081/api/v1' }
  else draft.value = { owner_user_id: '', name: '', level_id: 'basic' }
}
async function save() {
  saving.value = true; error.value = ''
  try {
    const { kind, id } = modal.value
    let body = { ...draft.value }
    if (kind === 'users') {
      body.bio_soup = {}
      for (const row of body.soup) {
        if (!row.element || row.element in body.bio_soup) throw new Error('物质不能为空或重复')
        body.bio_soup[row.element] = String(row.amount)
      }
      delete body.soup
    }
    if (kind === 'models') { try { body.composition = JSON.parse(body.composition) } catch { throw new Error('模型结构 JSON 格式无效') } }
    const options = { method: id ? 'PUT' : 'POST', body: JSON.stringify(body) }
    if (kind === 'servers') { const result = await request(`/admin/servers${id ? `/${encodeURIComponent(id)}` : ''}`, options); if (result.registration_key) issuedKey.value = { id: body.id, key: result.registration_key }; tickets.clear() }
    else if (kind === 'worlds') await remote('/worlds', options)
    else await request(`/admin/records/${kind}${id ? `/${id}` : ''}`, options)
    modal.value = null; notice.value = '已保存'; await load()
  } catch (e) { error.value = e.message } finally { saving.value = false }
}
async function remove() {
  saving.value = true; error.value = ''
  try {
    const id = deleteRow.value.id
    if (view.value === 'worlds') await remote(`/worlds/${id}`, { method: 'DELETE' })
    else await request(view.value === 'servers' ? `/admin/servers/${id}` : `/admin/records/${view.value}/${id}`, { method: 'DELETE' })
    deleteRow.value = null; tickets.clear(); notice.value = '已删除'; if (records.value.length === 1 && offset.value) offset.value = Math.max(0, offset.value - 50); await load()
  } catch (e) { error.value = e.message } finally { saving.value = false }
}
async function worldStatus(row) {
  error.value = ''; saving.value = true
  try { await remote(`/worlds/${row.id}`, { method: 'PATCH', body: JSON.stringify({ status: row.status === 'active' ? 'paused' : 'active' }) }); await load() }
  catch(e) { error.value = e.message } finally { saving.value = false }
}
async function inspect(row) {
  error.value = ''; saving.value = true
  try { details.value = { title: row.name, value: await remote(`/worlds/${row.id}`) } }
  catch(e) { error.value = e.message } finally { saving.value = false }
}
function download() {
  const url = URL.createObjectURL(new Blob([JSON.stringify(records.value, null, 2)], { type: 'application/json' }))
  const a = document.createElement('a'); a.href = url; a.download = `life-paper-${target.value || 'central'}-logs.json`; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000)
}
async function rotateKey() {
  saving.value = true; error.value = ''
  try {
    const row = resetKeyRow.value
    const result = await request(`/admin/servers/${encodeURIComponent(row.id)}/registration-key`, { method: 'POST' })
    issuedKey.value = { id: row.id, key: result.registration_key }; resetKeyRow.value = null; tickets.clear(); await load()
  } catch (e) { error.value = e.message } finally { saving.value = false }
}
async function copyKey() { try { await navigator.clipboard.writeText(issuedKey.value.key); notice.value = '密钥已复制' } catch { error.value = '无法访问剪贴板，请在密钥框中选择文本复制' } }
const filteredComponents = computed(() => catalog.value.components.filter(r => JSON.stringify(r).toLowerCase().includes(search.value.toLowerCase())))
const filteredElements = computed(() => catalog.value.elements.filter(r => JSON.stringify(r).toLowerCase().includes(search.value.toLowerCase())))
const componentName = c => c.name || c.id
</script>

<template>
  <main v-if="!connected" class="login-screen">
    <form class="login-form" @submit.prevent="login">
      <div class="brand"><ShieldCheck :size="26" /> LIFE PAPER <span>ADMIN</span></div>
      <h1>服务器管理</h1>
      <label>中心 API 地址<input v-model="base" type="url" required autocomplete="url" /></label>
      <label>管理员令牌<input v-model="token" type="password" required autocomplete="off" /></label>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <button class="primary" :disabled="busy">{{ busy ? '连接中…' : '登录后台' }}</button>
    </form>
  </main>
  <div v-else class="admin-layout">
    <aside>
      <div class="brand"><ShieldCheck :size="23" /><div>LIFE PAPER<small>服务器管理</small></div></div>
      <nav><button v-for="item in nav" :key="item.id" :class="{ active: view === item.id }" @click="switchView(item.id)"><component :is="item.icon" :size="18" />{{ item.name }}</button></nav>
      <div class="connection"><span class="online-dot"></span>中心已连接<small>{{ base }}</small></div>
      <button class="logout" @click="logout"><LogOut :size="17" />退出登录</button>
    </aside>
    <section class="workspace">
      <header><div><p class="eyebrow">管理控制台</p><h1>{{ title }}</h1></div><span class="count">{{ total }} {{ isLog ? '条记录' : '项' }}</span><button class="icon" title="刷新" aria-label="刷新" :disabled="busy" @click="load"><RefreshCw :size="19" :class="{ spinning: busy }" /></button></header>
      <div class="toolbar">
        <form class="search" @submit.prevent="resetLoad"><Search :size="17" /><input v-model="search" placeholder="搜索" aria-label="搜索" /><button type="submit">查询</button></form>
        <select v-if="isLog || view === 'worlds'" v-model="target" aria-label="日志来源或游戏服" @change="resetLoad"><option v-if="isLog" value="">中心服务</option><option v-if="view === 'worlds' && !fleet.length" value="">暂无游戏服</option><option v-for="s in fleet" :key="s.id" :value="s.id">{{ s.name }} · {{ s.id }}</option></select>
        <template v-if="isLog"><select v-model="category" aria-label="日志类别" @change="resetLoad"><option value="">所有类别</option><option value="network">网络请求</option><option value="runtime">运行日志</option><option value="audit">管理操作</option><option value="relay">中继转发</option></select><select v-model="level" aria-label="日志级别" @change="resetLoad"><option value="">所有级别</option><option>ERROR</option><option>WARN</option><option>INFO</option><option>DEBUG</option><option>TRACE</option></select><label class="check"><input v-model="autoRefresh" type="checkbox" />自动刷新</label><button class="icon" title="导出当前日志" aria-label="导出当前日志" @click="download"><Download :size="18" /></button></template>
        <button v-if="!isLog && view !== 'catalog'" class="primary new-button" :disabled="view === 'worlds' && !target" @click="edit()"><Plus :size="17" />新增</button>
      </div>
      <p v-if="error" class="error banner" role="alert"><CircleAlert :size="18" />{{ error }}</p>
      <p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <div class="table-scroll" v-if="view !== 'catalog'">
        <table v-if="view === 'users'"><thead><tr><th>ID</th><th>用户</th><th>邮箱</th><th>能量币</th><th>金币</th><th>解锁组件</th><th class="actions">操作</th></tr></thead><tbody><tr v-for="row in records" :key="row.id"><td class="mono">{{ row.id }}</td><td class="strong">{{ row.username }}</td><td>{{ row.email }}</td><td>{{ row.energy_coins }}</td><td>{{ row.gold_coins }}</td><td>{{ row.unlocked_components.length }}</td><td class="actions"><button class="icon" title="编辑用户与资产" aria-label="编辑用户与资产" @click="edit(row)"><Pencil :size="17" /></button><button class="icon danger" title="删除用户" aria-label="删除用户" @click="deleteRow = row"><Trash2 :size="17" /></button></td></tr></tbody></table>
        <table v-else-if="view === 'models'"><thead><tr><th>名称</th><th>模型 ID</th><th>所属用户</th><th>组件数</th><th>更新时间</th><th class="actions">操作</th></tr></thead><tbody><tr v-for="row in records" :key="row.id"><td class="strong">{{ row.name }}</td><td class="mono">{{ row.id }}</td><td>{{ row.owner_user_id }}</td><td>{{ Object.keys(row.composition.components || {}).length }}</td><td>{{ new Date(row.updated_at).toLocaleString() }}</td><td class="actions"><button class="icon" title="编辑模型" aria-label="编辑模型" @click="edit(row)"><Pencil :size="17" /></button><button class="icon danger" title="删除模型" aria-label="删除模型" @click="deleteRow = row"><Trash2 :size="17" /></button></td></tr></tbody></table>
        <table v-else-if="view === 'servers'"><thead><tr><th>游戏服</th><th>Server ID</th><th>API 地址</th><th>接入状态</th><th class="actions">操作</th></tr></thead><tbody><tr v-for="row in records" :key="row.id"><td class="strong">{{ row.name }}</td><td class="mono">{{ row.id }}</td><td>{{ row.api_url }}</td><td><span class="status" :class="{ active: row.online }">{{ row.online ? '已验证 · 在线' : row.key_configured ? '等待游戏服接入' : '需生成密钥' }}</span></td><td class="actions"><button class="icon" title="查看日志" aria-label="查看日志" :disabled="!row.online" @click="target = row.id; switchView('logs')"><ScrollText :size="17" /></button><button class="icon" title="管理世界" aria-label="管理世界" :disabled="!row.online" @click="target = row.id; switchView('worlds')"><Globe :size="17" /></button><button class="icon" title="重置接入密钥" aria-label="重置接入密钥" @click="resetKeyRow = row"><KeyRound :size="17" /></button><button class="icon" title="编辑游戏服" aria-label="编辑游戏服" @click="edit(row)"><Pencil :size="17" /></button><button class="icon danger" title="移除游戏服登记" aria-label="移除游戏服登记" @click="deleteRow = row"><Trash2 :size="17" /></button></td></tr></tbody></table>
        <table v-else-if="view === 'worlds'"><thead><tr><th>世界</th><th>世界 ID</th><th>所属用户</th><th>关卡</th><th>状态</th><th class="actions">操作</th></tr></thead><tbody><tr v-for="row in records" :key="row.id"><td class="strong">{{ row.name }}</td><td class="mono">{{ row.id }}</td><td>{{ row.owner_user_id }}</td><td>{{ row.level_id }}</td><td><span class="status" :class="row.status">{{ row.status === 'active' ? '运行中' : '已暂停' }}</span></td><td class="actions"><button class="icon" title="查看生物实例" aria-label="查看生物实例" :disabled="saving" @click="inspect(row)"><Eye :size="17" /></button><button class="icon" :title="row.status === 'active' ? '暂停世界' : '恢复世界'" :aria-label="row.status === 'active' ? '暂停世界' : '恢复世界'" :disabled="saving" @click="worldStatus(row)"><component :is="row.status === 'active' ? Pause : Play" :size="17" /></button><button class="icon danger" title="删除世界" aria-label="删除世界" @click="deleteRow = row"><Trash2 :size="17" /></button></td></tr></tbody></table>
        <table v-else class="log-table"><thead><tr><th>时间 / ID</th><th>级别</th><th>类别</th><th>来源</th><th>内容</th></tr></thead><tbody><tr v-for="row in records" :key="row.id"><td class="log-time">{{ new Date(row.time).toLocaleString() }}<small>#{{ row.id }}</small></td><td><span class="level" :class="row.level">{{ row.level }}</span></td><td>{{ { network: '网络', runtime: '运行', audit: '管理' }[row.category] || row.category }}</td><td class="mono">{{ row.target }}</td><td><div v-if="row.category === 'network'" class="network-line"><b>{{ row.fields.method }}</b><span class="mono">{{ row.fields.path }}</span><b :class="{ 'error-text': row.fields.status >= 400 }">{{ row.fields.status }}</b><span>{{ row.fields.duration_ms }} ms</span></div><pre>{{ JSON.stringify(row.fields, null, 2) }}</pre></td></tr></tbody></table>
        <div v-if="!records.length" class="empty">{{ busy ? '加载中…' : error ? '未能加载数据' : '暂无记录' }}</div>
      </div>
      <div v-else class="catalog">
        <h2>组件 <span>{{ filteredComponents.length }}</span></h2>
        <table><thead><tr><th>ID</th><th>名称</th><th>类别</th><th>详情</th></tr></thead><tbody><tr v-for="item in filteredComponents" :key="item.id"><td class="mono">{{ item.id }}</td><td>{{ componentName(item) }}</td><td>{{ item.category }}</td><td><button class="icon" title="查看组件定义" aria-label="查看组件定义" @click="details = { title: componentName(item), value: item }"><Eye :size="17" /></button></td></tr></tbody></table>
        <h2>物质 <span>{{ filteredElements.length }}</span></h2>
        <table><thead><tr><th>ID</th><th>名称</th><th>类别</th><th>详情</th></tr></thead><tbody><tr v-for="item in filteredElements" :key="item.id"><td class="mono">{{ item.id }}</td><td>{{ item.name }}</td><td>{{ item.category }}</td><td><button class="icon" title="查看物质定义" aria-label="查看物质定义" @click="details = { title: item.name || item.id, value: item }"><Eye :size="17" /></button></td></tr></tbody></table>
      </div>
      <footer v-if="view === 'users' || view === 'models'"><span>{{ total ? offset + 1 : 0 }}–{{ Math.min(offset + 50, total) }} / {{ total }}</span><button class="icon" title="上一页" aria-label="上一页" :disabled="!offset || busy" @click="offset -= 50; load()"><ChevronLeft :size="18" /></button><button class="icon" title="下一页" aria-label="下一页" :disabled="offset + 50 >= total || busy" @click="offset += 50; load()"><ChevronRight :size="18" /></button></footer>
      <footer v-if="isLog"><span>最近 {{ records.length }} 条</span><button :disabled="busy || !logBefore" @click="logBefore = null; load()">最新记录</button><button :disabled="busy || !nextBefore || records.length < 100" @click="logBefore = nextBefore; load()">更早记录</button></footer>
    </section>
    <div v-if="modal" class="overlay" @click.self="!saving && (modal = null)"><section class="dialog" role="dialog" aria-modal="true" :aria-label="modal.id ? '编辑' : '新增'">
      <div class="dialog-head"><h2>{{ modal.id ? '编辑' : '新增' }}{{ title }}</h2><button class="icon" title="关闭" aria-label="关闭" :disabled="saving" @click="modal = null"><X :size="20" /></button></div>
      <form @submit.prevent="save"><div class="form-body">
        <template v-if="modal.kind === 'users'">
          <div class="form-grid"><label>用户名<input v-model="draft.username" required maxlength="64" /></label><label>邮箱<input v-model="draft.email" type="email" required maxlength="254" /></label><label>{{ modal.id ? '新密码（留空保留）' : '密码' }}<input v-model="draft.password" type="password" :required="!modal.id" minlength="8" maxlength="1024" autocomplete="new-password" /></label></div>
          <h3>资产</h3><div class="form-grid"><label>能量币<input v-model="draft.energy_coins" inputmode="numeric" pattern="[0-9]+" required /></label><label>金币<input v-model="draft.gold_coins" inputmode="numeric" pattern="[0-9]+" required /></label></div>
          <div class="subhead"><h3>生物汤</h3><button type="button" class="icon" title="添加物质" aria-label="添加物质" @click="draft.soup.push({ element: '', amount: '0' })"><Plus :size="17" /></button></div>
          <div v-for="(row,i) in draft.soup" :key="i" class="soup-row"><select v-model="row.element" required aria-label="物质"><option value="" disabled>选择物质</option><option v-for="element in catalog.elements" :key="element.id" :value="element.id">{{ element.name }} · {{ element.id }}</option></select><input v-model="row.amount" inputmode="numeric" pattern="[0-9]+" required aria-label="数量" /><button type="button" class="icon danger" title="移除物质" aria-label="移除物质" @click="draft.soup.splice(i,1)"><Trash2 :size="17" /></button></div>
          <h3>解锁组件</h3><div class="unlock-list"><label v-for="component in catalog.components" :key="component.id" class="check"><input v-model="draft.unlocked_components" type="checkbox" :value="component.id" />{{ componentName(component) }}<small>{{ component.id }}</small></label></div>
        </template>
        <template v-else-if="modal.kind === 'models'"><div class="form-grid"><label>模型名称<input v-model="draft.name" required maxlength="128" /></label><label>所属用户 ID<input v-model="draft.owner_user_id" inputmode="numeric" pattern="[0-9]+" required /></label></div><label>模型结构<textarea v-model="draft.composition" class="code-input" spellcheck="false" required rows="18"></textarea></label></template>
        <template v-else-if="modal.kind === 'servers'"><label>Server ID<input v-model="draft.id" :disabled="!!modal.id" pattern="[A-Za-z0-9_-]{1,128}" required /></label><label>名称<input v-model="draft.name" required maxlength="128" /></label><label>游戏服 API 地址<input v-model="draft.api_url" type="url" required /></label></template>
        <template v-else><label>世界名称<input v-model="draft.name" required maxlength="128" /></label><label>所属用户 ID<input v-model="draft.owner_user_id" required inputmode="numeric" pattern="[0-9]+" /></label><label>关卡标识<input v-model="draft.level_id" required maxlength="64" /></label></template>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
      </div><div class="dialog-actions"><button type="button" :disabled="saving" @click="modal = null">取消</button><button class="primary" :disabled="saving">{{ saving ? '保存中…' : '保存' }}</button></div></form>
    </section></div>
    <div v-if="deleteRow" class="overlay" @click.self="!saving && (deleteRow = null)"><section class="dialog compact" role="alertdialog" aria-modal="true" aria-label="确认删除"><div class="dialog-head"><h2>确认删除</h2><button class="icon" title="关闭" aria-label="关闭" :disabled="saving" @click="deleteRow = null"><X :size="20" /></button></div><div class="form-body"><p>删除「{{ deleteRow.name || deleteRow.username || deleteRow.id }}」？</p><p v-if="view === 'users'" class="muted">账号、中心资产和模型将被删除。独立游戏服存档需单独管理。</p><p v-else-if="view === 'worlds'" class="muted">世界存档及生物实例代码将被永久删除。</p><p v-else-if="view === 'servers'" class="muted">移除登记后，游戏服进程和存档仍保留。</p><p v-if="error" class="error">{{ error }}</p></div><div class="dialog-actions"><button :disabled="saving" @click="deleteRow = null">取消</button><button class="destructive" :disabled="saving" @click="remove">{{ saving ? '删除中…' : '删除' }}</button></div></section></div>
    <div v-if="resetKeyRow" class="overlay"><section class="dialog compact" role="alertdialog" aria-modal="true" aria-label="重置接入密钥"><div class="dialog-head"><h2>重置接入密钥</h2></div><div class="form-body"><p>{{ resetKeyRow.name }} · {{ resetKeyRow.id }}</p><p class="muted">原密钥将失效，游戏服需要使用新密钥重新注册。</p><p v-if="error" class="error">{{ error }}</p></div><div class="dialog-actions"><button :disabled="saving" @click="resetKeyRow = null">取消</button><button class="primary" :disabled="saving" @click="rotateKey">生成新密钥</button></div></section></div>
    <div v-if="issuedKey" class="overlay"><section class="dialog compact" role="dialog" aria-modal="true" aria-label="游戏服接入密钥"><div class="dialog-head"><h2>游戏服接入密钥</h2><button class="icon" title="关闭" aria-label="关闭" @click="issuedKey = null"><X :size="20" /></button></div><div class="form-body"><p>{{ issuedKey.id }}</p><label>验证密钥<input class="mono" :value="issuedKey.key" readonly @focus="$event.target.select()" /></label><p class="muted">仅本次显示。用于游戏服配置的 registration.verification_key。</p><p v-if="error" class="error">{{ error }}</p></div><div class="dialog-actions"><button @click="copyKey"><Copy :size="16" />复制密钥</button><button class="primary" @click="issuedKey = null">完成</button></div></section></div>
    <div v-if="details" class="overlay" @click.self="details = null"><section class="dialog" role="dialog" aria-modal="true" :aria-label="details.title"><div class="dialog-head"><h2>{{ details.title }}</h2><button class="icon" title="关闭" aria-label="关闭" @click="details = null"><X :size="20" /></button></div><div class="form-body"><pre class="detail-json">{{ JSON.stringify(details.value, null, 2) }}</pre></div></section></div>
  </div>
</template>
