# Life Paper API

> 版本：`v1`（设计稿）  
> 基础地址：`/api/v1`  
> 当前服务端已实现部分业务路由，其余接口仍为设计稿。

## 目录

- [接口约定](#接口约定)
- [统一错误格式](#统一错误格式)
- [账户相关](#账户相关)
  - [注册](#注册)
  - [登录](#登录)
  - [退出登录](#退出登录)
  - [注销账号](#注销账号)
  - [获取账号信息](#获取账号信息)
  - [修改账号信息](#修改账号信息)
- [世界相关](#世界相关)
  - [获取世界列表](#获取世界列表)
  - [获取或创建新手世界](#获取或创建新手世界)
  - [创建玩家世界](#创建玩家世界)
  - [暂停世界](#暂停世界)
  - [恢复世界](#恢复世界)
  - [获取世界地层生成配置](#获取世界地层生成配置)
- [服务器相关](#服务器相关)
  - [获取服务器信息](#获取服务器信息)
  - [获取服务器状态](#获取服务器状态)
- [生物组件相关](#生物组件相关)
  - [获取全部生物组件](#获取全部生物组件)
  - [获取已解锁生物组件](#获取已解锁生物组件)
  - [获取未解锁生物组件](#获取未解锁生物组件)
  - [解锁生物组件](#解锁生物组件)
- [玩家数据相关](#玩家数据相关)
  - [获取玩家基础信息](#获取玩家基础信息)
  - [获取玩家详情信息](#获取玩家详情信息)
  - [获取玩家货币与生物汤](#获取玩家货币与生物汤)
  - [获取玩家已有生物组件](#获取玩家已有生物组件)
  - [获取玩家生物列表](#获取玩家生物列表)
- [生物模型相关](#生物模型相关)
  - [创建生物模型](#创建生物模型)
  - [获取自己的生物模型](#获取自己的生物模型)
  - [浏览模型文件夹](#浏览模型文件夹)
  - [创建模型文件夹](#创建模型文件夹)
  - [读取模型文件](#读取模型文件)
  - [上传模型文件](#上传模型文件)
  - [删除模型文件或文件夹](#删除模型文件或文件夹)
  - [重命名模型文件或文件夹](#重命名模型文件或文件夹)
  - [删除生物模型](#删除生物模型)
- [神经网络相关](#神经网络相关)
  - [上传模型](#上传模型)
  - [删除模型](#删除模型)
  - [获取模型关联实例](#获取模型关联实例)
- [生物实例相关](#生物实例相关)
  - [生成生物实例](#生成生物实例)
  - [杀死生物实例](#杀死生物实例)
  - [读取生物状态](#读取生物状态)
  - [获取生物视角](#获取生物视角)

## 接口约定

- 数据格式：请求和响应默认使用 `application/json`。
- 认证：登录后返回 `token`，需要认证的请求携带 `Authorization: Bearer <token>`。
- 标识符：资源 ID 使用 UUID 字符串；时间使用 ISO 8601 UTC 字符串。
- 列表：支持 `page`（从 1 开始）和 `page_size`（默认 20，最大 100）。
- 成功响应：`{ "data": ... }`；创建成功使用 `201`，删除成功使用 `204`。
- 修改接口使用 `PATCH` 表示部分字段更新，字段未传入时保持原值。

## 统一错误格式

```json
{
  "error": {
    "code": "RESOURCE_NOT_FOUND",
    "message": "生物模板不存在",
    "request_id": "req_01H..."
  }
}
```

常见状态码：`400` 参数错误、`401` 未登录、`403` 无权限、`404` 资源不存在、`409` 状态冲突、`500` 服务端错误。

## 账户相关

### 注册

`POST /auth/register`

请求：`{ "username": "alice", "password": "...", "email": "alice@example.com" }`  
响应 `201`：`{ "data": { "user_id": 1, "username": "alice" } }`

### 登录

`POST /auth/login`

请求：`{ "username": "alice", "password": "..." }`  
响应 `200`：`{ "data": { "token": "session_token", "expires_at": "2026-..." } }`

### 退出登录

`POST /auth/logout`（需要认证）

使当前 token 失效。响应：`204 No Content`。

### 注销账号

`DELETE /account`（需要认证）

先撤销该账号的全部登录会话，再删除当前账号及其可删除的数据。其他设备持有的 token 也会立即失效。响应：`204 No Content`。

### 获取账号信息

`GET /account/me`（需要认证）

请求头：`Authorization: Bearer <token>`
响应 `200`：`{ "data": { "id": 1, "username": "alice", "email": "alice@example.com" } }`

用于验证当前 token 是否有效，并取得当前登录账号的基础信息。token 无效、过期或对应账号不存在时返回 `401`。

### 修改账号信息

## 世界相关

> 以下接口均需要认证：`Authorization: Bearer <token>`。

世界当前由服务端按用户隔离管理。一个用户可以拥有多个世界，每个世界有独立的 UUID、关卡 ID 和运行状态。

世界摘要格式：

```json
{
  "id": "world-uuid",
  "kind": "player_created",
  "name": "新手世界",
  "owner_user_id": 1,
  "level_id": "tutorial",
  "status": "active"
}
```

`kind` 表示世界的来源，当前可能是：

- `player_created`：为玩家创建的独立世界，包括新手世界
- `server_provided`：由服务器提供的世界

新手世界不是单独的 `kind`，而是通过 `level_id: "tutorial"` 识别。
当前世界创建接口只会创建 `player_created` 世界；服务器世界的创建和管理接口尚未开放。

`status` 当前可能是：

- `active`：参与游戏循环
- `paused`：暂停模拟，不参与游戏循环

### 获取世界列表

`GET /worlds`

返回当前登录用户拥有的全部世界：

```json
{
  "data": [
    {
      "id": "world-uuid",
      "kind": "player_created",
      "name": "新手世界",
      "owner_user_id": 1,
      "level_id": "tutorial",
      "status": "active"
    }
  ]
}
```

### 获取或创建新手世界

`POST /worlds/tutorial`

当前用户没有新手世界时创建一个；已经存在时返回原来的新手世界。因此重复调用是幂等的。

成功响应：`200 OK`，格式为：

```json
{
  "data": {
    "id": "world-uuid",
    "kind": "player_created",
    "name": "新手世界",
    "owner_user_id": 1,
    "level_id": "tutorial",
    "status": "active"
  }
}
```

### 创建玩家世界

`POST /worlds`

请求：

```json
{
  "level_id": "custom_level_1",
  "name": "我的第一个世界"
}
```

`name` 是世界的显示名称，可省略；省略时默认使用 `level_id`。名称最长 128 个字符。

可选 `ai_full_visibility: true` 启用简单模式的 AI 世界查询权限；默认 `false`。此设置随世界保存，新手世界始终开放完整查询。具体权限见 [AI 接口说明](ai.md)。

成功响应：`200 OK`，返回新创建的世界摘要。

当前限制：

- `level_id` 不能为空，首尾空白会被去除。
- `level_id` 最长 64 个字符。
- 单个用户最多拥有 16 个世界。
- 服务端最多保留 1024 个世界。

违反输入或数量限制时返回 `400 BAD_REQUEST`。

### 暂停世界

`POST /worlds/{world_id}/pause`

只能暂停当前用户拥有的世界。暂停成功后返回状态为 `paused` 的世界摘要。

启用磁盘世界存储时，暂停会保存世界快照，并释放该世界的运行时内存。

### 恢复世界

`POST /worlds/{world_id}/resume`

只能恢复当前用户拥有的世界。恢复成功后返回状态为 `active` 的世界摘要。

如果世界之前已被卸载，恢复时会从磁盘快照重新加载世界运行时。

### 获取世界地层生成配置

`GET /worlds/{world_id}/ground`（需要认证）

返回客户端生成地层所需的确定性生成器参数，以及服务器分发的地形规则和视觉样式。客户端不得自行修改这些参数；世界当前只提供读取接口。

暂停或服务重启后尚未加载到内存的世界也可读取，服务端使用同一世界目录从存档读取配置，不会因此启动世界循环或创建替代世界。

响应示例：

```json
{
  "data": {
      "generator": {
      "seed": 12345,
      "noise_scale_px": 384,
      "variation_percent": 100,
      "profile": {
        "cliff": "cliff",
        "valley": "valley",
        "swamp": "swamp",
        "pond": "pond",
        "grassland": "grassland",
        "wetland": "wetland",
        "wall": "wall"
      },
      "area": null
    },
    "perlin_permutations": [[...256 个字节...], [...256 个字节...], [...256 个字节...], [...256 个字节...]],
    "default_terrain": "wasteland",
    "terrain_layers": [],
    "terrains": [
      {
        "id": "grassland",
        "name": "草地",
        "slows_movement": false,
        "blocks_movement": false,
        "generates_oxygen": true,
        "generated_matter_recipe": "grass_matter",
        "visual": { "color": "#6fa85b", "pattern": "grass" }
      }
    ]
  }
}
```

## 服务器相关

### 获取服务器信息

### 获取服务器状态

## 生物组件相关

生物组件定义来自服务端注册表。组件摘要格式如下：

```json
{
  "id": "photosensor_organ_1",
  "name": "感光器官I",
  "category": "external_organ",
  "slots": {
    "body": 1
  },
  "function_code": null,
  "unlock": { "default_unlocked": true, "methods": [] }
}
```

`category` 当前可能是 `external_organ`、`body`、`internal_organ`。
`slots` 表示该组件可以连接的各类别组件数量；没有槽位的类别不会出现在对象中。
`function_code` 预留给后续功能实现，目前可能为 `null`。

组件解锁状态按用户保存。数据库中没有该用户组件解锁记录时，组件视为未解锁。
现有内置组件默认已解锁；后续组件可以配置能量币、金币或成就解锁条件。

### 获取全部生物组件

`GET /organism-components`

不需要认证，返回服务端当前注册的全部组件。

响应 `200`：

```json
{
  "data": [
    {
      "id": "photosensor_organ_1",
      "name": "感光器官I",
      "category": "external_organ",
      "slots": { "body": 1 },
      "function_code": null
    }
  ]
}
```

### 获取已解锁生物组件

`GET /organism-components/unlocked`（需要认证）

只返回当前登录用户已经解锁的组件。无已解锁组件时返回空数组，而不是 `404`。

### 获取未解锁生物组件

`GET /organism-components/locked`（需要认证）

返回当前登录用户尚未解锁的组件。该列表等价于“全部组件”减去“已解锁组件”。

### 解锁生物组件

`POST /organism-components/{component_id}/unlock`（需要认证）

尝试解锁指定组件。多个解锁方式之间是 OR 关系，满足任意一种即可；扣除余额与写入解锁记录在同一个数据库事务中完成，重复调用不会重复扣费。

成功响应 `200`：

```json
{
  "data": {
    "component": {
      "id": "advanced_sensor_1",
      "name": "高级感知器",
      "category": "external_organ",
      "slots": {},
      "function_code": null,
      "unlock": {
        "default_unlocked": false,
        "methods": [
          { "type": "energy_coins", "amount": 100 },
          { "type": "gold_coins", "amount": 10 }
        ]
      }
    },
    "energy_coins": 900,
    "gold_coins": 0
  }
}
```

余额不足返回 `409 RESOURCE_CONFLICT`。组件不存在返回 `404 RESOURCE_NOT_FOUND`。配置为成就解锁的组件会检查当前用户的成就记录，未完成时返回 `409`，不会扣除任何货币。

## 玩家数据相关

### 获取玩家基础信息

### 获取玩家货币与生物汤

`GET /player/data`（需要认证）

返回当前玩家的账户级货币和生物汤余额。余额均为非负整数；新玩家初始余额为 `0`。

```json
{
  "data": {
    "energy_coins": 0,
    "gold_coins": 0,
    "bio_soup": [
      { "element_id": "carbon", "name": "碳", "symbol": "C", "amount": 0 },
      { "element_id": "hydrogen", "name": "氢", "symbol": "H", "amount": 0 },
      { "element_id": "oxygen", "name": "氧", "symbol": "O", "amount": 0 }
    ]
  }
}
```

- `energy_coins`：能量币，通过普通生存或任务获得。
- `gold_coins`：金币，通过充值或交易获得。
- `bio_soup`：生物汤，按元素 ID 分开保存，在游戏过程中获得；元素 ID 与元素组件和世界物质系统共用注册表。

当前基础元素包括碳、氢、氧。后续新增基础元素并注册到元素系统后，接口会自动返回该元素的生物汤余额。

### 获取玩家详情信息

### 获取玩家已有生物组件

### 获取玩家生物列表

## 生物模型相关

以下接口均需要认证。模型只能由拥有者查看和修改；对不属于当前用户的模型统一返回 `404`，避免泄露模型是否存在。

### 创建生物模型

`POST /organism-models`

```json
{
  "name": "基础觅食生物",
  "components": [
    { "id": "10000000-0000-0000-0000-000000000001", "component_id": "small_body_1" },
    { "id": "10000000-0000-0000-0000-000000000002", "component_id": "photosensor_organ_1" }
  ],
  "connections": [
    {
      "first": "10000000-0000-0000-0000-000000000001",
      "second": "10000000-0000-0000-0000-000000000002"
    }
  ]
}
```

模型名称为 1 至 128 个字符，最多包含 256 个组件实例。`components` 的首项必须是身体类别，模型至少包含一个身体，且所有组件必须处于同一连接图中；允许多个身体通过具有相应槽位的中间组件连接。组件必须存在且已由当前用户解锁；每条连接会同时占用双方与对方类别对应的一个槽位，双方都必须有空闲的匹配槽位。成功返回 `201` 和模型记录，`composition` 保存通过验证的完整构成。

### 获取自己的生物模型

`GET /organism-models`

按创建时间倒序返回当前用户的模型列表及完整 `composition`。

### 浏览模型文件夹

`GET /organism-models/{model_id}/folder?path=src`

`path` 可省略，表示模型根目录。响应条目包含 `name`、相对 `path`、`kind`（`file` 或 `folder`）以及文件 `size`。

### 创建模型文件夹

`POST /organism-models/{model_id}/folders`

```json
{ "path": "src/controllers" }
```

会同时创建缺失的父目录。路径必须是模型目录内的相对路径。

### 读取模型文件

`GET /organism-models/{model_id}/files?path=src/main.py`

返回指定文件的原始内容。路径必须是模型目录内的安全相对路径，文件不存在时返回 `404`。

### 上传模型文件

`PUT /organism-models/{model_id}/files?path=src/main.py`

请求体直接传输文件原始字节，文件不存在时创建，已存在时覆盖。服务端会创建缺失的父目录，单文件最大 `10 MiB`。

### 删除模型文件或文件夹

`DELETE /organism-models/{model_id}/entries?path=src/old`

文件夹会连同其中内容递归删除。不允许删除模型根目录。`..`、绝对路径和其他可能离开模型目录的路径均会返回 `400`。

### 重命名模型文件或文件夹

`PATCH /organism-models/{model_id}/entries`

```json
{
  "path": "src/old.txt",
  "name": "new.txt"
}
```

`name` 只能是单个文件或文件夹名称，不能包含路径分隔符。成功响应 `204 No Content`。

### 删除生物模型

`DELETE /organism-models/{model_id}`

删除数据库模型记录及其整个文件目录，成功响应 `204 No Content`。

## 神经网络相关

### 上传模型

### 删除模型

### 获取模型关联实例

## 生物实例相关

### 生成生物实例

`POST /api/v1/worlds/{world_id}/organisms`，需要 Bearer 登录凭证。

```json
{ "model_id": "模型 UUID" }
```

成功返回 `201 Created`，响应为 `{ "data": OrganismInstance }`。只能向自己的世界放置自己的模型。世界不存在返回 404，无世界权限返回 403，模型不存在或不属于当前用户返回 404。暂不接受坐标、方向、耐久或容器内容等客户端覆盖参数。

实例包含：

- `id`：独立实例 UUID；重复创建同一模型会产生不同实例。
- `model_id`、`owner_user_id`、`name`：来源模型和归属信息。
- `position: { "x": 0, "y": 0 }`：根身体中心的世界坐标，单位 px；出生时若与阻挡地形重叠，服务器会有界搜索附近合法位置。
- `rotation_mdeg: 0`：实例方向，单位千分之一度，顺时针为正。
- `motion`：连续位置、弧度方向、质心线速度和角速度，字段与单位见 [AI 文档](ai.md)；旧实例尚未推进时可为 `null`。
- `composition`：模型结构的独立快照，保留组件、宿主、连接、尺寸、局部方向；根身体位置归零。
- `component_states`：以模型内组件 UUID 为键的运行状态，组件 ID 在各实例内独立使用。

每个组件状态包含 `properties`（创建时的服务端属性快照）、`durability`（初始为最大耐久）、`energy_mj`（实例创建时为该组件的能量容量，之后由游戏刻消耗）、`container`：

```json
{
  "non_gas_capacity_mg": 2000,
  "gas_capacity_ul": 20000,
  "non_gas_contents_mg": {},
  "gas_contents_ul": {}
}
```

非气体以毫克计，气体以微升计，内含物质按元素注册 ID 保存；新实例容器为空。小型躯体初始耐久 100，容量 2g / 20mL；反应结构增加自身 20g 容器，气室结构提供自身 40mL 容器。已有实例保留属性快照，不自动扩容。`properties` 还包含最大耐久、损坏/修复权重、质量、储能上限、容量、反应速率上下限和传感器槽位。设计中未指定的质量为 `null`。组件目录接口也下发这些可配置属性。

`non_gas_contents_mg` 当前支持浮点mg，以保存范围吸收的混合物比例和微量余数；旧存档的整数值兼容。世界新增可选 `matter` 存档字段，保存逻辑tick及圆形吸收/补入记录；旧世界缺少时以满默认营养初始化。AI `get_tile` 返回 `matter_mg` 映射，各项为该格当前剩余质量。草地恢复、衰减和记录字段见[世界物质设计](matter.md)。

实例随世界存档保存；创建立即写入存档，服务重启后仍可恢复。暂停世界也允许创建和查询实例，不会自动恢复世界。实例保存模型和属性快照，之后删除源模型或调整组件配置不改变已有实例。创建接口暂不扣除资源，不执行伤害或反应。活动世界的移动由游戏 tick 统一推进，支持喷射推力、角速度、惯性和地形约束。

### 获取世界内自己的生物实例

`GET /api/v1/worlds/{world_id}/organisms`，需要登录且拥有该世界。

返回 `200 OK`，响应为 `{ "data": [OrganismInstance] }`。客户端进入世界时读取列表，添加成功后使用服务端返回的实例渲染。

### 实例 AI 代码

创建实例时复制模型目录中的全部文件及子目录，保存至配置文件同级的 `data/organism_instances/{实例UUID}/`。`composition.id` 和 `composition.program_id` 均改为实例 UUID，`model_id` 保留来源模型 UUID。复制为独立文件，修改或删除模型、修改其他实例不会影响已生成实例。

入口固定为根目录 `main.py`；源模型缺少入口时补充含 `def main(game)` 的示例入口。复制失败或实例存档失败时不创建实例，回滚新代码目录。单文件最大 10 MiB，复制总量最多 50 MiB、1024 个目录项，不接受符号链接或 Windows 重解析点。代码目录与世界存档均需备份。

以下接口需要 Bearer 登录凭证，并验证世界、实例归属。路径通过 `path` 指定，必须是目录内的相对路径。

| 方法 | 路径 | 内容 |
| --- | --- | --- |
| GET | `/api/v1/worlds/{world_id}/organisms/{organism_id}/folder?path=` | 获取目录，返回 `{ "data": [{ "name", "path", "kind", "size" }] }` |
| GET | `/api/v1/worlds/{world_id}/organisms/{organism_id}/files?path=main.py` | 读取文件原文 |
| PUT | 同上 | 原始文件内容请求体，保存成功返回 204，最大 10 MiB |
| POST | `/api/v1/worlds/{world_id}/organisms/{organism_id}/folders` | `{ "path": "lib" }` 创建目录，返回 204 |
| PATCH | `/api/v1/worlds/{world_id}/organisms/{organism_id}/entries` | `{ "path": "brain.py", "name": "helpers.py" }` 重命名，返回 204 |
| DELETE | `/api/v1/worlds/{world_id}/organisms/{organism_id}/entries?path=brain.py` | 删除文件或目录，返回 204 |

实例的根目录 `main.py` 可编辑、覆盖，不能删除或重命名。暂停世界也可编辑，重启后从磁盘读取已保存的修改。旧存档中的实例首次访问代码时创建独立空白 `main.py`，不引用或复制当前模型代码，以免将模型之后的修改意外套用到旧实例。

客户端从世界右侧的生物列表进入实例 AI 编辑器，默认打开 `main.py`，编辑后可保存代码。世界模拟时每 tick 自动调用 `main(game)`，日志和错误从世界生物列表的日志入口查看。代码可读取自身、查询世界，并使用实例独立的 `game.memory` 保存记忆。

### 查询 AI 调用器及实例状态

`GET /api/v1/worlds/{world_id}/organisms/{organism_id}/ai`，需登录并拥有世界与实例。

返回 `{ "data": { "enabled": false, "entry_file": "main.py", "entry_function": "main(game)", "timeout_ms": 2000, "max_queries": 256, "workers": 2, "cache_entries": 8, "state": { "calls": 0, "memory": {}, "pending_commands": [], "last_run": null, "history": [] } } }`。`history` 按时间顺序保存最近的运行报告，客户端可轮询该接口追加显示新日志。

### 自动执行与运行报告

世界模拟时由服务端每 tick 自动执行 Python `main(game)`，暂停时停止执行。查询 AI 状态可获取以下运行报告：

```json
{
  "data": {
    "calls": 1,
    "memory": { "calls": 1 },
    "pending_commands": [],
    "last_run": {
      "ok": true,
      "logs": "organism wall 1\n",
      "error": null,
      "duration_ms": 8,
      "commands": [],
      "metrics": {
        "code_version": "SHA-256内容版本",
        "snapshot_cache_hit": true,
        "code_cache_hit": true,
        "worker_reused": true,
        "worker_pid": 12345,
        "snapshot_us": 1500,
        "startup_us": 0,
        "load_us": 900,
        "execution_us": 20,
        "query_us": 0,
        "roundtrip_us": 6500,
        "total_us": 8000
      }
    }
  }
}
```

脚本报错、超时或查询超限时，`last_run.ok` 为 false，`error` 为诊断，保留运行前的 memory。每次提交的调用增加 calls 并随世界存档保存。

Python SDK、可见范围、配置与运行限制详见 [AI 接口说明](ai.md)。

调用器使用常驻 Python 进程池、按内容版本缓存的代码快照及编译缓存。每次调用重新创建用户模块，持久状态通过 `memory` 保存。`metrics` 耗时单位为微秒，阶段存在包含关系，不能直接相加；旧存档没有指标时按默认值读取。批量地格接口为 `game.world.get_tiles(points)`，缓存命中不消耗查询额度，批量未缓存坐标按地格数消耗额度。

`game.me.components[component_UUID]` 可按组件能力调用 `jet(material, rate, angle=0)`、`absorb_air(rate)` 或 `absorb_matter(rate)`，组件 UUID 沿用模型。Rust 校验参数后原样记录请求速率和偏转角，成功完成 AI 调用后保存到 `pending_commands` 和本次 `last_run.commands`；同一组件最后一条输出覆盖前面的输出，包括速率为零的控制。连续调用 `jet("water", 31)`、`jet("water", 0)` 最终只保存速率为 0 的一条记录。一次喷射指令示例：

```json
{
  "component_id": "00000000-0000-0000-0000-000000000001",
  "action": { "method": "jet", "material": "water", "rate_mg_per_tick": 31, "angle_deg": 3 }
}
```

吸气指令的 `action.method` 为 `absorb_air`，使用 `rate_ml_per_tick`，范围 0～5 mL/tick，精细度10，tick 执行时向下取整为0.5 mL一档；`absorb_matter` 仍使用 `rate_mg_per_tick`，0～50 mg/tick、5 mg一档。吸气已接入地层许可、身体气体容量、耐久和能耗限制，空气按21%氧气、79%氮气入库，深水及阻挡地层禁用且不扣世界库存。非气体吸收以器官为圆心、半径5 px，从世界范围库存按平均组成吸收并扣除余量，实际量受容器、体积、耐久和能量约束。详见 [AI说明](ai.md)。成功且无输出时清空待执行指令；执行失败或保存失败时保留原有待执行指令，失败报告的 `commands` 为空。喷射执行时按档位、耐久、储能和库存限制扣料，产生推力及转矩。`get_vision` 预留并明确报未实现。组件请求和世界查询共享 `max_queries` 预算。完整动力学规则见 [设计文档](dynamics.md)。

### 世界物质交换

以下接口需要登录且拥有世界，结果包装为 `{ "data": ... }`。

- `GET /api/v1/worlds/{world_id}/matter?x=0&y=0&radius_px=5[&since=123]`：首次返回附近活动记录，携带since时只返回新增/有效期更新的活动记录。响应含 `tick`、`cursor`、`reset`、查询区域、恢复/流失参数和 `records`。半径0～64px；since不能超过当前历史条数。
- `GET /api/v1/worlds/{world_id}/matter/history?after=0&limit=100`：分页读取历史，含气体和过时记录。响应含 `tick`、`cursor`、`has_more`、`records`。limit为1～200。

记录使用世界tick，带生物/器官来源、交换实际量、圆形范围、过时tick。`sequence` 是稳定的世界内记录序号，`updated_at_sequence` 是最后一次修改有效期时的历史游标。客户端增量按sequence覆盖，按tick过时；更换查询范围须重新取不带since的快照。同期有效期变化也会随增量返回，不能只按新记录序号追加。完整语义与存储限制见[世界物质设计](matter.md)。

### Python API 文档

`GET /api/v1/python-api` 无需登录，返回 `{ "data": { "version": 2, "title": "Python API", "sections": [...], "components": [...], "substances": [...] } }`。接口条目包含签名、可用状态、参数、返回值、执行规则和示例；物质与组件目录来自当前服务器注册表。客户端侧栏「API 文档」加载此接口，提供搜索、分类、复制示例和刷新。

喷射使用 `part.jet(material, rate, angle=0)`，`angle_deg` 范围 -5°～5°，精细度10，每档1°，取最近整数度，半档远离零，正值顺时针。缺少角度的旧调用与存档默认0°。执行时从连接身体扣料，质量/体积限制同时生效，按实际扣除质量产生推力；空仓无推力。固液排入喷口所在格，气体散入环境。

### 杀死生物实例

### 读取生物状态

### 获取生物视角
