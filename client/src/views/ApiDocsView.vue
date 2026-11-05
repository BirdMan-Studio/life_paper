<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { BookOpen, Copy, RefreshCw, Search } from 'lucide-vue-next'
import { pythonApi, getApiBase } from '../api/http'

const document = ref(null)
const loading = ref(false)
const error = ref('')
const search = ref('')
const tab = ref('api')
const selected = ref('')
const copyStatus = ref('')
let request = 0
let copyTimer
const matches = value => JSON.stringify(value).toLowerCase().includes(search.value.trim().toLowerCase())
const sections = computed(() => (document.value?.sections || []).map(section => ({ ...section, entries: section.entries.filter(matches) })).filter(section => section.entries.length))
const entries = computed(() => sections.value.flatMap(section => section.entries))
const active = computed(() => entries.value.find(entry => entry.signature === selected.value) || entries.value[0])
const substances = computed(() => (document.value?.substances || []).filter(matches))
const components = computed(() => (document.value?.components || []).filter(matches))
async function refresh() {
  const current = ++request
  loading.value = true
  error.value = ''
  try {
    const response = await pythonApi.documentation()
    if (current !== request) return
    if (!Array.isArray(response.data?.sections) || !Array.isArray(response.data?.substances) || !Array.isArray(response.data?.components)) throw new Error('服务器返回的文档格式无效')
    document.value = response.data
  } catch (cause) { if (current === request) error.value = cause.message }
  finally { if (current === request) loading.value = false }
}
async function copy() {
  try { await navigator.clipboard.writeText(active.value.example); copyStatus.value = '已复制' }
  catch { copyStatus.value = '无法访问剪贴板' }
  clearTimeout(copyTimer)
  copyTimer = setTimeout(() => { copyStatus.value = '' }, 2200)
}
onMounted(refresh)
onBeforeUnmount(() => { request++; clearTimeout(copyTimer) })
</script>

<template>
  <main class="api-docs">
    <header class="docs-heading"><BookOpen :size="24" /><h2>API 文档</h2><small v-if="document">v{{ document.version }}</small><button class="icon-btn" title="刷新文档" aria-label="刷新文档" :disabled="loading" @click="refresh"><RefreshCw :size="17" /></button></header>
    <div class="docs-source">{{ getApiBase() }}/python-api</div>
    <div class="docs-toolbar">
      <div class="docs-tabs" role="tablist" aria-label="文档目录"><button v-for="item in [['api', 'Python 接口'], ['substances', '物质'], ['components', '组件']]" :key="item[0]" role="tab" :aria-selected="tab === item[0]" :class="{ active: tab === item[0] }" @click="tab = item[0]">{{ item[1] }}</button></div>
      <label class="docs-search"><Search :size="16" /><input v-model="search" type="search" aria-label="搜索文档" placeholder="搜索接口、物质或组件" /></label>
    </div>
    <p v-if="error" class="docs-error" role="alert">{{ error }} <button @click="refresh">重试</button></p>
    <p v-if="loading && !document" class="docs-empty" role="status">正在读取文档…</p>
    <template v-else-if="document">
      <div v-if="tab === 'api' && active" class="docs-workspace">
        <nav class="docs-index" aria-label="Python 接口目录"><section v-for="section in sections" :key="section.id"><h3>{{ section.title }}</h3><button v-for="entry in section.entries" :key="entry.signature" :class="{ selected: active.signature === entry.signature }" @click="selected = entry.signature"><code>{{ entry.signature }}</code><span v-if="entry.status === 'planned'">待实现</span></button></section></nav>
        <article class="docs-article">
          <span class="docs-status" :class="{ planned: active.status === 'planned' }">{{ active.status === 'planned' ? '待实现' : '可用' }}</span>
          <h3><code>{{ active.signature }}</code></h3><p>{{ active.description }}</p>
          <div v-if="active.parameters?.length" class="docs-table-wrap"><table><thead><tr><th>参数</th><th>类型</th><th>范围与单位</th></tr></thead><tbody><tr v-for="parameter in active.parameters" :key="parameter[0]"><td><code>{{ parameter[0] }}</code></td><td>{{ parameter[1] }}</td><td>{{ parameter[2] }}</td></tr></tbody></table></div>
          <h4>返回值</h4><p>{{ active.returns }}</p><h4 v-if="active.notes?.length">执行规则</h4><ul><li v-for="note in active.notes" :key="note">{{ note }}</li></ul>
          <div class="docs-code-heading"><h4>示例</h4><span role="status">{{ copyStatus }}</span><button class="icon-btn" title="复制示例" aria-label="复制示例" @click="copy"><Copy :size="16" /></button></div><pre><code>{{ active.example }}</code></pre>
        </article>
      </div>
      <div v-else-if="tab === 'substances' && substances.length" class="docs-table-wrap"><table><thead><tr><th>物质</th><th>ID</th><th>组成</th><th>化学能 mJ/mg</th><th>处理速度</th></tr></thead><tbody><tr v-for="substance in substances" :key="substance.id"><td>{{ substance.name }}</td><td><code>{{ substance.id }}</code></td><td>{{ substance.symbol }}</td><td>{{ substance.chemistry?.chemical_energy_mj_per_mg ?? '—' }}</td><td>{{ Math.round((substance.initial_processing_speed ?? 1) * 100) }}%</td></tr></tbody></table></div>
      <div v-else-if="tab === 'components' && components.length" class="docs-table-wrap"><table><thead><tr><th>组件</th><th>ID</th><th>耐久</th><th>储能 mJ</th><th>非气体 mg</th><th>气体 mL</th></tr></thead><tbody><tr v-for="component in components" :key="component.id"><td>{{ component.name }}</td><td><code>{{ component.id }}</code></td><td>{{ component.properties?.max_durability }}</td><td>{{ component.properties?.energy_capacity_mj }}</td><td>{{ component.properties?.non_gas_capacity_mg }}</td><td>{{ (component.properties?.gas_capacity_ul || 0) / 1000 }}</td></tr></tbody></table></div>
      <p v-else class="docs-empty">没有匹配的条目</p>
    </template>
  </main>
</template>

<style scoped>
.docs-article td:nth-child(-n+2){white-space:nowrap}
.api-docs{padding:24px;min-width:0;color:var(--text,#dce6ee)}.docs-heading{display:flex;align-items:center;gap:12px}.docs-heading h2{font-size:24px;margin:0}.docs-heading small{color:var(--muted);flex:1}.docs-source{font:11px monospace;color:var(--muted);margin:12px 0 20px;overflow-wrap:anywhere}.docs-toolbar{display:flex;gap:20px;justify-content:space-between;border-bottom:1px solid var(--line);padding-bottom:12px}.docs-tabs{display:flex;gap:4px}.docs-tabs button{border:0;background:transparent;color:var(--muted);padding:9px 14px;border-bottom:2px solid transparent}.docs-tabs button.active{color:var(--cyan);border-color:var(--cyan)}.docs-search{display:flex;gap:8px;align-items:center;border:1px solid var(--line);padding:0 10px;min-width:0}.docs-search input{background:transparent;color:inherit;border:0;padding:9px 0;min-width:0;width:210px;outline:none}.docs-workspace{display:grid;grid-template-columns:260px minmax(0,1fr);gap:28px}.docs-index{padding:18px 14px 20px 0;border-right:1px solid var(--line)}.docs-index h3{font-size:12px;color:var(--muted);margin:16px 0 8px}.docs-index button{display:block;width:100%;border:0;border-left:2px solid transparent;background:transparent;color:inherit;text-align:left;padding:10px;overflow-wrap:anywhere}.docs-index button.selected{background:rgba(89,217,230,.08);border-color:var(--cyan);color:var(--cyan)}.docs-index code{font-size:11px}.docs-index span{display:block;font-size:10px;color:#d8c173;margin-top:4px}.docs-article{padding:26px 0;min-width:0;line-height:1.8;font-size:13px}.docs-article h3{font-size:18px;margin:12px 0;overflow-wrap:anywhere}.docs-article h4{font-size:13px;margin:22px 0 8px}.docs-article p{margin:10px 0}.docs-article ul{padding-left:18px}.docs-article li{margin:8px 0}.docs-status{color:#9bdaae;font-size:11px}.docs-status.planned{color:#d8c173}.docs-table-wrap{overflow-x:auto;max-width:100%;margin-top:20px}table{width:100%;border-collapse:collapse;text-align:left;font-size:12px}th,td{padding:12px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--muted);font-weight:500}td code{overflow-wrap:anywhere}.docs-code-heading{display:flex;align-items:center;gap:10px}.docs-code-heading h4{flex:1}.docs-code-heading span{font-size:11px;color:var(--muted)}pre{background:#0a131b;border:1px solid var(--line);padding:16px;overflow:auto;line-height:1.7;font-size:12px;max-width:100%}.docs-error{color:#f59c9c;padding:12px 0}.docs-error button{background:transparent;color:inherit;border:1px solid currentColor;padding:4px 10px;margin-left:12px}.docs-empty{padding:36px 0;color:var(--muted)}@media(max-width:760px){.api-docs{padding:16px}.docs-toolbar{flex-direction:column;gap:12px}.docs-search input{width:100%}.docs-workspace{grid-template-columns:minmax(0,1fr);gap:0}.docs-index{border-right:0;border-bottom:1px solid var(--line);max-height:220px;overflow:auto}.docs-article h3{font-size:16px}th,td{padding:10px 8px}}
</style>
