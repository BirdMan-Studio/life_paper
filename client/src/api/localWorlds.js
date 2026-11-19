function invoke(command, args) {
  const bridge = window.__TAURI_INTERNALS__
  if (!bridge?.invoke) throw new Error('请在桌面客户端中打开本地存档')
  return bridge.invoke(command, args)
}

export const localWorlds = {
  list: mode => invoke('local_world_list', { mode }),
  catalog: (source, entries) => invoke('local_tutorial_catalog', { source, entries: entries ?? null }),
  async connect(mode, signal) {
    signal?.throwIfAborted()
    const connection = await invoke('local_world_start', { mode })
    signal?.throwIfAborted()
    const base = new URL(connection.api_url)
    if (base.protocol !== 'http:' || base.hostname !== '127.0.0.1' || base.pathname !== '/api/v1') throw new Error('本地游戏引擎地址无效')
    const deadline = Date.now() + 10000
    while (Date.now() < deadline) {
      signal?.throwIfAborted()
      try {
        const timeout = AbortSignal.timeout(500)
        const response = await fetch(`${base}/game-server`, { signal: signal ? AbortSignal.any([signal, timeout]) : timeout, redirect: 'error' })
        const metadata = await response.json()
        if (metadata?.data?.auth_mode !== 'local' || metadata.data.server_id !== connection.server_id) throw new Error('本地游戏引擎身份不匹配')
        return connection
      } catch (error) {
        if (signal?.aborted) throw error
        if (error.message === '本地游戏引擎身份不匹配') throw error
        await new Promise(resolve => setTimeout(resolve, 100))
      }
    }
    throw new Error('本地游戏引擎启动超时')
  },
}
