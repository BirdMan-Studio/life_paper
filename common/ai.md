# 生物 Python AI 接口

入口文件为实例代码目录根部的 `main.py`，定义同步函数 `main(game)`。不需要使用 `this`、继承基类或编写无限循环。返回值暂时忽略。

```python
def main(game):
    me = game.me
    tile = game.world.get_tile(me.position.x, me.position.y)
    game.memory["visits"] = game.memory.get("visits", 0) + 1
    game.log(me.name, tile.terrain, game.memory["visits"])
```

每次调用重新加载已保存的实例代码及其辅助模块，模块全局变量不会跨调用保留。使用 `game.memory` 保存长期记忆。支持在同一代码目录中创建模块并使用 `import brain` 等标准导入。

## game.me

当前生物实例的操作入口，支持属性访问与字典索引。读取状态得到调用开始时的只读快照，组件方法用于提交控制输出。

| 接口 | 含义 |
| --- | --- |
| `game.me.id`、`model_id`、`name` | 实例与来源模型信息 |
| `game.me.position.x`、`.y` | 根身体中心的世界坐标，整数 px |
| `game.me.rotation` | 顺时针角度，度 |
| `game.me.rotation_mdeg` | 原始千分之一度方向 |
| `game.me.components[component_UUID]` | 使用模型阶段确定的 UUID 获取当前实例的组件对象，可读状态及调用对应能力的方法 |
| `game.me.composition` | 创建时的结构、根身体、组件放置和连接快照 |

组件包含 `id`、`component`（组件类型 ID）、`placement`、`durability`、`energy_mj`、`properties`、`container`、`api`。容器包含容量及内含物质映射：`non_gas_capacity_mg`、`gas_capacity_ul`、`non_gas_contents_mg`、`gas_contents_ul`。组件 UUID 使用字符串；相同模型生成的多个生物沿用模型的组件 UUID，调用始终作用于当前生物实例。

```python
def main(game):
    for component_id, component in game.me.components.items():
        game.log(component_id, component.component, component.durability)
        game.log(component.container.non_gas_contents_mg)
```

嵌套状态同样只读，列表转换为 tuple；直接写 `game.me.position.x = 10` 或 `part.durability = 0` 会报错。

## 组件控制

依据 `设计.txt` 的 API 大纲，组件对象只提供其具备的能力方法；躯体、皮肤、反应结构、传感器组和气室目前没有大纲规定的主动控制方法，仍可读取状态。

| 组件 | 方法 | 参数与返回值 |
| --- | --- | --- |
| 喷射器官 I | `jet(material, rate)` | 注册物质 ID、0～200 mg/tick；原样记录并返回请求速率 |
| 吸收器官 I | `absorb_air(rate)` | 0～50 mg/tick；原样记录并返回请求速率 |
| 吸收器官 I | `absorb_matter(rate)` | 0～50 mg/tick；原样记录并返回请求速率 |
| 感光器官 I | `get_vision(angles=None)` | 预留，当前抛出 `NotImplementedError`；等待光照与遮挡模拟接入 |

```python
def main(game):
    # 替换为模型装配阶段生成并保存的组件 UUID。
    jet = game.me.components["喷射器官组件UUID"]
    absorber = game.me.components["吸收器官组件UUID"]
    requested = jet.jet("water", 31)  # 记录 31 mg/tick，不取整、不喷射
    jet.jet("water", 0)              # 覆盖为 0；最终仅保存这条控制
    absorber.absorb_matter(13)        # 原样记录 13 mg/tick
    game.log("requested", requested)
```

速率使用有限整数或浮点数，不接受 bool、NaN、Infinity、负数或超过上限的数。Rust 只校验组件归属、所支持的方法、注册物质及参数范围，原样记录速率，不进行精细度取整，也不根据当前耐久执行或拒绝控制。未知物质或非法速率抛出 `ValueError`，越权组件抛出 `PermissionError`。调用不存在的方法抛出 `AttributeError`。每次组件桥接请求消耗一个 `max_queries` 额度，和世界查询共用预算。

同一组件每次 AI 调用只保留最后一条有效控制；吸收空气和吸收物质共享该输出位置。成功调用将本次全部输出替换至实例 `ai.pending_commands`，并记录在 `ai.last_run.commands`，和记忆一起保存；成功但没有输出时清空待执行指令。脚本异常、超时、协议错误或存档失败不提交本次控制，原有已保存的待执行指令保留。队列按生物实例隔离，旧存档缺少该字段时读取为空列表。

当前完成的是 API、参数校验和控制输出存储，尚未执行实际喷射、吸收、耗能及移动。方法返回的是原始请求速率，不表示实际完成量。未来世界 Tick 执行时负责精细度取整、组件耐久检查、消耗能量、扣除喷射物质、限制吸收容量，并处理深水禁用吸气及耐久效率。一次待执行输出应在 Tick 中消费后清除，不能反复重放。

`component.api` 提供服务器下发的能力参数，数值保存在组件的 `properties.api` 中，包括最大速率、精细度分段数、基础及最大可变耗能；内置默认值集中于 `server/src/domain/organism_component/control.rs`。旧实例缺少这些参数时兼容内置默认值，显式空能力对象可关闭主动能力。感光角度暂按列表预留：不填表示均匀采样，填写时为 1～120 个 -30°～30° 相对角度；角度公式形式仍待确认，当前不会返回模拟视野数据。

## game.world

| 接口 | 含义 |
| --- | --- |
| `id`、`name`、`level_id` | 当前世界信息 |
| `mode` | `tutorial`、`simple` 或 `normal` |
| `capabilities.full_map` | 是否允许查询任意地格 |
| `capabilities.full_organisms` | 是否允许查询所有生物概要 |
| `get_tile(x, y)` | 查询一个地层像素，由 Rust 世界逻辑生成结果 |
| `get_tiles(points)` | 一次查询 1～256 个 `(x, y)` 坐标，返回保持输入顺序和重复项的 tuple |
| `get_organisms(offset=0, limit=100)` | 分页读取可见生物概要，返回 tuple |

`get_tile` 坐标必须为有符号 32 位整数，结果含 `x`、`y`、`terrain`（地形注册 ID）、`height`、`moisture`、`nutrients`、`temperature`。四种地层属性均为 0～100。

自身所在地格随调用快照预取，重复地格查询在本次调用内缓存。`get_tiles` 对尚未缓存的坐标去重后一次发送给 Rust；查询额度按实际请求的新地格数量计算，缓存命中不消耗额度。批量请求中有不可见地格时，整批失败。缓存每次调用重建，不会沿用上次世界状态。

```python
def main(game):
    x, y = game.me.position.x, game.me.position.y
    if game.world.capabilities.full_map:
        tiles = game.world.get_tiles([(x, y), (x + 1, y), (x, y + 1)])
        for tile in tiles:
            game.log(tile.x, tile.y, tile.terrain)
```

生物概要只包含 `id`、`name`、`position`、`rotation`，不包含其他实例的 AI 记忆、代码或内部组件状态。分页 `offset >= 0`，`1 <= limit <= 100`，无结果返回空 tuple。

教程世界开放完整查询。创建自定义世界时设置 `ai_full_visibility: true` 可启用简单模式；该设置随世界存档保存。普通模式目前只允许查询自身所在像素，生物概要列表只含自己，之后再按感知组件扩展。查询不可见地格抛出 `PermissionError`；非法参数抛出 `TypeError` 或 `ValueError`，不会伪装成空地格。

世界和自身数据为同一次调用开始时的快照，权限由服务端逐次校验。修改客户端代码中的对象或直接发送桥接请求无法扩大游戏接口的查询范围。

## 记忆、时间与日志

自动 AI 调度按 `ai.workers` 限制并发数量，在世界之间轮流分配空闲 worker。同一世界每 tick 的所有 AI 读取同一份只读快照；该世界本 tick 完成后才能开始下一 tick，不会与自己的下一 tick 重叠。世界独立计时（目标间隔 1 秒），慢世界不会形成全局 tick 屏障；所有 worker 忙碌时任务等待空闲容量，不无限创建运行任务。暂停世界不再派发任务，已派发的结果提交仍校验暂停状态。

每 tick 根据生物已安装且耐久大于零的组件扣除基础能耗，无论 AI 是否输出指令或执行失败。喷射器官基础 5 mJ、吸收器官基础 3 mJ、皮肤和感光器官各 2 mJ；按组件 API 配置读取。控制指令只叠加力度对应的可变能耗，不重复扣基础能耗。优先使用根身体储能，不足时使用其他组件储能；暂停且未提交的调用不消耗能量。

- `game.memory`：当前实例的可变 dict，使用字符串键和可 JSON 序列化的值，不接受 NaN、Infinity；序列化后最多 16 KiB。成功执行才提交修改；异常、超时或保存失败时保留原有记忆。每个实例独立，随世界存档保存。
- `game.time.step`：该实例本次调用的序号，从 1 开始，包含已保存的失败调用次数，不代表世界 Tick 或已经推进的模拟时间。
- `game.tutorial`：教程世界中为含 `level_id` 的只读对象，其他模式为 `None`。目标和进度接口待教程系统接入。
- `game.log(*values)` 和 `print(...)`：记录日志。普通错误保留错误发生前的日志；日志截断为最多 16384 字符。超时或协议中断可能无法取回 Python 缓冲的日志。

Python 包 `life_game` 在运行时注入，需要类型提示时可写 `from life_game import Game`。SDK 源码位于 `server/python/life_game.py`，示例入口位于 `server/python/main.py`。

## 开启与调用

服务器安装 Python 3.10+，在实际加载的 `config.toml` 添加：

```toml
[ai]
enabled = true
python = "python"
timeout_ms = 2000
max_queries = 256
workers = 2
cache_entries = 8
```

`python` 可填写解释器绝对路径，不能包含额外命令行参数。Windows 路径可用 TOML 单引号，例如 `python = 'C:\Python312\python.exe'`。`timeout_ms` 范围 50～60000；`max_queries` 范围 1～4096。

`workers` 控制常驻进程数与最大并发数，范围 1～16，默认 2；启动时预热。`cache_entries` 控制服务端代码快照缓存和每个进程的编译缓存条目数，范围 1～64，默认 8；按最近使用淘汰。旧配置不添加这两项也可继续使用。

保存实例代码后，世界模拟会在每 tick 自动调用 AI；暂停时停止执行。状态和日志通过 `GET /api/v1/worlds/{world_id}/organisms/{organism_id}/ai` 获取，客户端可从世界生物列表查看日志。接口与响应见 [HTTP API](api.md)。

每次调用在代码文件锁下计算全部文件的 SHA-256 内容版本，版本未变时复用代码快照，修改入口、辅助模块或资源后创建新快照；释放文件锁后执行，期间保存的修改从下次调用生效。常驻 Python 进程复用编译后的代码对象，每次仍创建新的入口和辅助模块，实例间不会共享用户模块的全局变量。支持普通包和命名空间包。每次调用使用独立临时工作目录；读取随代码保存的资源请通过 `Path(__file__).parent` 定位，跨调用状态保存在 `game.memory`。

同一实例不可并发执行；调度器按进程池容量并发运行，容量不足时排队等待。默认导入及执行期限为 2 秒，包含桥接查询等待；需要补建进程时，启动握手也单独受相同期限限制。代码哈希与快照准备不计入执行期限，但计入报告总耗时。默认最多请求 256 个新地格或生物分页查询，批量请求按地格数计费。超时、协议错误或遗留后台线程的进程会被终止，后续调用重新创建进程。

运行报告包含 `metrics`：`code_version`、快照及代码缓存命中状态、进程复用状态与 PID，以及微秒单位的 `snapshot_us`、`startup_us`、`load_us`、`execution_us`、`query_us`、`roundtrip_us`、`total_us`。`duration_ms` 为总耗时的毫秒值。Python 执行时间包含查询等待，`roundtrip_us` 包含整个协议交互，多个阶段存在包含关系，不能直接相加。

执行默认关闭。此实现是用于可信代码的独立 CPython 子进程调用器，使用隔离模式并清除继承的环境变量，但不是操作系统沙箱；代码仍拥有服务端账号的文件和网络权限，也没有内存或子进程树隔离。公开服务器需要先配置操作系统隔离，再开启不可信玩家代码执行。

已有 `main.py` 不会被覆盖；旧的顶层脚本需改为 `def main(game)` 后才能使用调用器。
