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

### 杀死生物实例

### 读取生物状态

### 获取生物视角
