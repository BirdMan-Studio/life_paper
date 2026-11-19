# 管理后台

## 启动

中心服务与一体服务均提供 `/admin/`，独立游戏服不托管后台页面。

在 `client` 目录执行 `npm run build`。从仓库根目录启动中心：

```powershell
cargo run -p server --bin central-server
```

没有设置 `LIFE_PAPER_ADMIN_TOKEN`（或值为空）时，首次启动自动生成256位随机管理员令牌，保存在中心配置同级的 `data/admin/admin-token.txt`，后续启动复用。生成发生在数据库连接之前。启动日志只显示本地文件路径，不输出令牌内容；该文件不通过HTTP提供，必须由本机用户查看。仓库默认配置下，从仓库根目录查看：

```powershell
Get-Content .\server\data\admin\admin-token.txt
```

显式设置 `LIFE_PAPER_ADMIN_TOKEN` 时优先使用环境变量（32～256个可见ASCII字符），不会覆盖本地令牌文件；此时在启动服务的PowerShell中用 `$env:LIFE_PAPER_ADMIN_TOKEN` 查看生效值。移除环境变量并重启后恢复使用本地文件。损坏或无效的本地文件会使启动失败，不会静默替换；需要重置时，停止服务、删除该文件并重启即可重新生成。

访问 `http://127.0.0.1:8080/admin/`，填写中心 API 地址及令牌。浏览器仅在当前标签的 `sessionStorage` 中保存令牌，退出登录后清除。普通玩家账号没有管理权限。公网管理应使用HTTPS。本地令牌文件包含完整管理凭证，应限制访问、避免分享或提交；Unix新建文件权限为0600，Windows继承所在目录的访问权限。

发布时把 `client/dist` 的**完整内容**放在中心配置同级的 `admin-dist` 目录，或用 `LIFE_PAPER_ADMIN_DIST` 指向它的绝对路径。仓库开发时会自动查找 `client/dist`。页面需要的脚本和图标均为本地构建资源。

## 管理范围

| 页面 | 操作 |
| --- | --- |
| 用户与资产 | 搜索、分页、新增/修改/删除用户；重置密码；修改能量币、金币、生物汤；授予/移除组件解锁 |
| 生物模型 | 跨用户查询、新增、修改名称/归属/结构、删除；结构编辑使用JSON，保存时校验组件、连接槽位及几何 |
| 游戏服 | 持久登记、修改、移除游戏服；生成/重置接入密钥、显示验证在线状态；切换其日志和世界管理 |
| 世界管理 | 查看所有玩家的世界、新增世界、暂停/恢复、查看生物实例及其AI状态、删除世界及实例代码 |
| 日志 | 查询中心和所选游戏服的运行、HTTP网络、管理操作日志；级别/类别/关键字筛选、历史翻页、5秒自动刷新、导出当前结果 |
| 物质与组件 | 查看当前编译版本的完整定义；这些定义仍由Rust注册表维护，暂不支持后台动态修改物理和物质规则 |

资产与解锁修改在同一个数据库事务中提交，金额以十进制字符串传递，避免浏览器大整数精度损失。重置密码、删除用户前撤销其Redis会话；Redis不可用时拒绝这些操作。游戏票据仍有最多15分钟有效期，撤销中心会话不会立即撤销已发给游戏服的票据。

模型编辑不改变已经生成的生物实例快照，不修改模型代码文件。新模型沿用现有默认入口生成逻辑。删除中心账号不会远程删除独立游戏服存档，需在世界管理中分别处理。世界当前支持修改运行状态，名称、所属用户、关卡在创建后保持不变。

## 远程游戏服

先在后台“游戏服”登记 `server_id`、名称及完整API地址，例如 `https://game.example.com/api/v1`。新增时中心生成接入密钥，页面仅显示一次；中心只保存摘要。将其写入游戏服 `[registration].verification_key`，填写 `central_api_url`、与登记一致的 `public_api_url`，并固定中心公钥，再按[拆分部署说明](deployment.md)启动新版 `game-server`。旧登记需先重置生成密钥。

游戏服启动时携带接入密钥向中心注册，之后每30秒心跳；中心确认密钥、ID及登记地址一致才标记在线。90秒无心跳则不再发放新的连接票据，后台也不能申请新的日志管理票据。中心重启后在线状态清空，等待重新上报。

后台向中心申请用途为 `game-admin`、仅限已验证在线游戏服、有效期15分钟的管理票据，再由浏览器直接访问游戏服的 `/api/v1/management/...`。管理员主令牌只发给中心，普通游戏票据无法访问管理接口。游戏服本地验签，并向中心发送注册/心跳，不连接PostgreSQL或Redis。浏览器必须能访问登记的游戏服地址；HTTPS后台应连接HTTPS游戏服。本地离线模式拒绝远程管理接口。

登记不会启动或关闭游戏进程；删除登记不会删除存档。移除登记或管理员退出后，已签发管理票据最长15分钟失效。

重置接入密钥会使旧密钥立即失效、清除在线状态；更新游戏服配置后重新启动。已签发玩家及管理票据仍最长15分钟失效。修改登记的对外地址也会清除在线状态，须同步修改游戏服 `public_api_url`。

## 日志

中心与游戏服分别写入自身 `data_dir/logs/events.jsonl`（中心为配置同级 `data/logs`）。单文件约10 MiB轮转，保留一个 `events.previous.jsonl`；查询窗口最多最近5000条，重启从这两个文件恢复。每页1～500条，默认100，按ID倒序，通过 `before` 查询更早记录。导出包含当前页面的筛选结果；完整保留日志可由运维另行采集JSONL文件。

- 运行日志：接入 `tracing`，级别由 `RUST_LOG` 控制，默认INFO。Python `game.log` 及执行报告仍随生物实例AI历史保存，可从世界详情查看。
- 网络日志：HTTP方法、路径、响应状态、服务端处理耗时、请求ID；响应带 `x-request-id`。不记录查询参数、请求/响应正文、Authorization或Cookie。此处是应用HTTP访问日志，不是网络抓包或反向代理连接日志。
- 管理日志：记录已认证的管理写操作、目标路径和返回状态，不记录修改正文、密码或票据。使用单一管理员令牌，因此审计身份为管理员，不区分多人操作人。

日志从部署新版进程后开始收集，不能补读旧进程此前仅输出到终端的内容。存档和日志目录不可由多个运行进程共用。

## API

所有中心接口均为 `/api/v1/admin` 前缀，Bearer凭证为环境变量指定或本地自动生成的管理员令牌；响应包装为 `{ "data": ... }`，删除数据记录返回204。

| 接口 | 含义 |
| --- | --- |
| `GET /session` | 校验管理员身份 |
| `GET /records/{users\|models}?search=&offset=0&limit=50` | 分页搜索，返回 `records/total/offset/limit` |
| `POST /records/{users\|models}` | 新增记录 |
| `PUT /records/{users\|models}/{id}` | 替换管理字段 |
| `DELETE /records/{users\|models}/{id}` | 删除记录 |
| `GET /catalog` | 物质和组件定义 |
| `GET /servers`、`POST /servers` | 查询、新增游戏服登记 |
| `PUT /servers/{id}`、`DELETE /servers/{id}` | 修改、删除登记 |
| `POST /servers/{id}/ticket` | 取得限定目标的管理票据 |
| `POST /servers/{id}/registration-key` | 重置接入密钥，返回仅显示一次的 `registration_key` |
| `GET /logs?level=&category=&search=&before=&limit=100` | 查询日志 |

用户正文：`username/email/password/energy_coins/gold_coins/bio_soup/unlocked_components`。金额和物质数量均为字符串，物质为注册ID到数量的映射，解锁项为组件ID数组；修改时密码留空保留。模型正文：`owner_user_id`字符串、`name`和`composition`对象。游戏服正文：`id/name/api_url`，ID不可修改。

新增游戏服响应含 `registration_key`，修改时不返回有效密钥。列表返回 `online/last_seen/key_configured`，不返回密钥或摘要。注册接口、玩家目录和世界地址查询见[API文档](api.md#独立游戏服接入)。

游戏服管理接口前缀 `/api/v1/management`，只接受 `game-admin` 签名票据：`GET /logs` 使用相同查询字段；`GET/POST /worlds`；`GET/PATCH/DELETE /worlds/{id}`。创建正文为 `owner_user_id`字符串、`name/level_id`；修改正文为 `{ "status": "active" | "paused" }`。世界详情返回生物实例及其AI历史。

## 验证

```powershell
cargo test -p server --offline
cargo test -p server postgres_admin_crud_and_asset_validation --offline -- --ignored
cargo build -p server --bins --offline
node server/tests/game-server-smoke.cjs
```

第二项需要已迁移的PostgreSQL，可用 `LIFE_PAPER_ADMIN_TEST_CONFIG` 指向中心TOML配置的绝对路径，默认读取server目录的 `config.toml`。它只创建并清理随机测试账号，不启动中心服务；Redis会话撤销不在该测试范围内。

客户端测试 `client/tests/admin.cjs` 使用Playwright，支持 `PLAYWRIGHT_MODULE` 和 `ADMIN_TEST_URL`，验证表单、凭证隔离、删除确认、世界管理和桌面/手机布局。
## 中继日志与远程管理

后台远程世界管理与游戏服日志默认直连，直连探测失败时使用鉴权进程内的中继，管理员主令牌仅发送中心，游戏服管理请求使用限定目标的 `game-admin` 票据。日志类别可选择“中继转发”（`relay`），在中心日志中查看转发目标、路径、响应状态和耗时；不会记录票据、正文或查询参数。写操作断线时提示结果未知，不自动重复提交，请刷新确认结果。

