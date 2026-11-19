# 中心服务与独立游戏服

中心服务可视化管理、游戏服登记及日志查询见[管理后台](admin.md)。

## 运行入口

同一 Rust workspace 提供三个可执行文件，共享领域模型、模拟、文件路径校验和 API 实现：

| 程序 | 职责 | 运行时依赖 |
| --- | --- | --- |
| `server` | 保留原有一体部署，兼容现有客户端与存档 | PostgreSQL、Redis；启用 AI 时需要 Python |
| `central-server` | 账号、会话、资产、解锁、模型管理，签发游戏票据及模型包 | PostgreSQL、Redis；不启动世界循环或 Python worker |
| `game-server` | 世界、物质、物理、生物实例、实例代码、Python AI、存档 | 本地可写目录；启用 AI 时需要 Python；联机模式向中心注册/心跳，不连接 PostgreSQL、Redis |

数据库代码目前仍在同一个 Rust library 中参与构建；这是进程、运行依赖和数据归属的拆分，尚未进一步拆成多个 Cargo package。

```powershell
cargo build -p server --release --bins
```

部署游戏服只需对应平台的 `game-server` 可执行文件、游戏服配置和可写的数据目录。Python worker 脚本嵌入程序，开启 AI 时还需要配置 Python 解释器及玩家代码使用的依赖。默认 AI 关闭，必须显式开启后才执行实例脚本。

## 中心服务

继续使用原 `server/config.toml`。配置路径解析和 `LIFE_PAPER_CONFIG` 与一体服一致。

```powershell
cargo run -p server --bin central-server
```

中心服务启动时在配置同级的 `data/signing-key.pk8` 自动生成并保存 Ed25519 私钥。私钥须持久保存并限制访问，不要分发给游戏服。多份中心服务若服务同一组游戏服，必须使用同一受保护的签名密钥。

获取公钥：

```powershell
Invoke-RestMethod http://127.0.0.1:8080/api/v1/auth/game-public-key
```

将返回的 `data.public_key` 写入游戏服配置。管理员应从可信中心取得并固定公钥，不由游戏服从客户端提供的地址临时下载公钥。

## 独立联机游戏服

先在中心后台“游戏服”新增登记，填写唯一 `server_id`、名称及客户端可访问的完整API地址。中心自动生成该游戏服的256位接入密钥，页面仅显示一次；中心只保存摘要。旧登记没有密钥时，点击“重置接入密钥”。

配置格式见 [game-server.example.toml](../server/game-server.example.toml)。为每台游戏服设置不同的 `server_id`、监听地址、端口和数据目录，填入中心公钥，并设置：

```toml
[registration]
central_api_url = "https://auth.example.com/api/v1"
public_api_url = "https://game.example.com/api/v1"
verification_key = "中心后台生成的游戏服接入密钥"
```

`public_api_url` 必须与中心登记一致，它可以是公网IP加端口，也可以是域名；不能把 `0.0.0.0` 监听地址当作对外地址。游戏服启动时携带密钥向中心注册，取得可由固定公钥验签的回执后才开放游戏API；注册失败则启动失败。运行时每30秒上报心跳和世界目录，超过90秒无心跳，中心不再发放新的玩家/管理票据或模型包。中心重启后须等待游戏服重新上报。

准备好 `server/game-server.toml` 后，从仓库根目录启动：

```powershell
$env:LIFE_PAPER_GAME_CONFIG = 'server/game-server.toml'
cargo run -p server --bin game-server
```

发布后可直接运行 `game-server.exe` 或 Linux 上的 `game-server`，不需要 Cargo。配置路径默认是工作目录的 `game-server.toml`；`data_dir` 相对配置文件目录解析。中心和游戏服可以部署在不同机器上，不需要共享目录。

游戏服提供 `/api/v1/game-server` 元数据及原 `/worlds/...` API、实例代码 API、`/python-api`。不提供注册、账号登录、中心资产、模型仓库或解锁接口。

客户端登录页的“连接设置”中：

- “中心 / 一体服务器地址”填写中心 API 地址，例如 `http://127.0.0.1:8080/api/v1`。
- 客户端仅配置中心地址，游戏服地址与中继地址由中心按世界返回，不再手动填写。
- 世界中枢提供单人、多人、教程三个入口；登录后不预查询世界目录。
- 多人入口从中心获取当前玩家的世界目录，不探测或请求游戏服。进入具体世界时才申请对应连接票据，并访问游戏服或中继；离开世界停止轮询并取消未完成请求。
- 单人和教程均读取桌面客户端磁盘存档。教程入口另从中心获取教程目录并缓存到磁盘，不查询在线游戏服或申请联机票据。
- 教程目录以横向科技树展示；第一次点击节点时从中心下载教程包（地图、事件、UI、示例生物模型及脚本），导入桌面壳启动的本地游戏服。地图模拟、脚本、事件判定、通关和 UI 状态均由本地游戏服处理，运行流量只发往回环地址。再次进入复用本地存档，不重复下载；目录请求连接失败时回退到磁盘缓存。
- 中心 `/game-servers` 响应携带 `deployment_mode`。`central` 使用上述目录流程；`combined` 沿用一体服世界API。

公网部署使用 HTTPS 或可信 TLS 反向代理保护票据和模型代码的传输。游戏服当前仍只允许玩家访问自己拥有的世界，多人共用一个世界的权限模型尚未改动。

## 身份与模型传递

1. 客户端在中心登录，中心会话仍由 Redis 管理。
2. 客户端向中心 `GET /game-servers` 查询已验证且在线的游戏服和玩家世界目录；进入世界时用中心会话请求 `GET /game-worlds/{world_id}/connection`。创建世界时请求所选游戏服的 `GET /game-servers/{id}/connection`。
3. 中心确认该游戏服已通过接入密钥验证且心跳有效，返回其已登记的 `api_url` 及限定游戏服、玩家、用途和15分钟有效期的票据。游戏服本地验签；客户端不会把中心会话令牌交给游戏服。
4. 创建实例前，客户端用中心会话请求 `POST /organism-models/{id}/export`，取得同样限定玩家和目标游戏服的签名模型包。
5. 客户端将模型包连同 `model_id`、可选 `initial_contents` 交给游戏服。游戏服校验签名、用途、目标、时间、归属和模型ID，创建本地独立快照及代码目录，之后不再依赖源模型。

票据与模型包使用带用途区分的 Ed25519 签名封装，格式为 `base64url(JSON).base64url(signature)`，不是 JWT。模型包保留文件夹和二进制资源；文件内容为无填充 base64url，文件及目录总计最多1024项；不另设单文件或单程序大小配额，代码写入和复制统一受当前服务器上每账号500 MiB累计配额限制。传输层为两次base64编码及JSON额外预留空间。导入拒绝绝对路径、越界路径和链接，失败时清理未发布的实例目录。

游戏票据校验不查中心会话，因此退出账号或删除账号不会即时撤销已经签发的票据，最长15分钟后失效；已存在的游戏服世界和实例不会因为中心账号删除而远程清除。后续若需要即时封禁、跨服资产结算或统一删档，应增加独立的管理协议。当前私人服结果不回写中心资产。

中心短时不可用时，已有世界继续模拟，未过期票据仍能访问游戏服；新登录、续票和新的模型导出需要中心在线。客户端提前30秒续票，游戏服返回401只清除游戏票据，不会直接注销中心账号。

世界选择入口使用 `GET /game-servers` 返回的当前玩家世界目录，再请求 `GET /game-worlds/{world_id}/connection` 获取目标游戏服地址及玩家票据。目录来自心跳，变更最多延迟约30秒；刚创建的世界在当前客户端缓存其连接票据，可以立即进入，无需等待下次心跳。

游戏服接入密钥用于中心验证游戏服身份，中心Ed25519公钥用于游戏服验证中心签发的票据，两者不可混用。接入密钥不发送给玩家客户端。重置接入密钥会立即清除在线状态，旧密钥无法再次注册；游戏服配置更新后重启即可恢复。已签发玩家与管理票据仍按原15分钟有效期失效，不立即撤销。

## 中继连接

中继目前运行在中心鉴权进程中。中心返回直连地址 `direct_url` 和相对中继地址 `relay_url`；客户端和管理后台先用无凭证的 `/game-server` 检测直连及游戏服身份，无法连接时通过中心转发游戏票据和请求。后续读请求连接失败时自动切换中继；写操作连接失败时提示结果未知，不自动重发，避免重复创建或修改数据。401、403等服务端错误不会触发回退。

中心必须能够访问登记的游戏服 `api_url`。这支持“客户端访问不到游戏服，中心能访问”的部署，尚不提供游戏服主动建立的反向隧道；只有内网地址且中心也访问不到的游戏服仍需端口映射或隧道。

中继仅支持已验证在线游戏服的游戏与管理API，分别校验 `game-entry` 和 `game-admin` 票据。不会转发账号会话、管理员令牌或Cookie，不跟随游戏服重定向。每中心进程最多8个并发请求，请求体和响应的传输上限均为1008 MiB，以容纳500 MiB原始文件及编码开销；账号实际文件配额仍为500 MiB。连接超时5秒，上游总超时30秒，繁忙返回503。中继事件记入中心日志的 `relay` 类别，网络日志照常记录。

## 本地运行基础

配置见 [game-local.example.toml](../server/game-local.example.toml)：`auth.mode = "local"`，监听地址必须为回环IP，使用自己生成的32～256字符令牌。该模式无数据库、无中心账号，固定本地玩家ID为1。

本地创建实例时提供 `local_model` 对象代替签名 `model_bundle`，字段为 `id`、`name`、`composition`、`files`（相对路径到base64url字节映射）及可选 `directories`。必须携带本地令牌。联机模式拒绝未经中心签名的模型包。

桌面客户端通过Tauri命令读取应用数据目录的 `worlds/singleplayer/data/worlds`（单人）和 `worlds/tutorial/data/worlds`（教程）。Windows默认根目录为 `%APPDATA%/dev.lifepaper.desktop`；具体位置由Tauri的 `app_data_dir()` 决定。教程目录缓存为 `worlds/tutorial/catalog.json`，记录中心地址以避免混用不同来源的目录。教程包导入后，原始定义、示例模型与运行进度随世界一起保存。两个模式目录独立，不按中心账号分隔。

仅查看单人列表或教程科技树不会启动引擎。进入或新建本地世界时，桌面壳自动启动 `game-server`，单人和教程各用一个进程、随机回环端口和独立随机令牌；客户端正常退出时回收它创建的进程。单人模式关闭Python AI；教程模式启用Python AI，需要 `python`（Python 3）在 PATH 中可用。教程可以添加包内提供的示例生物，并填写初始容器物质；示例脚本尝试喷射水，物质与运动教程需要在创建生物时填写水。中心模型导入仍未接入本地模式。现有登录仍使用中心账号，教程进度不回写中心。

开发时先执行 `cargo build -p server --bin game-server --offline`，桌面壳从仓库的 `target/debug` 查找引擎；发布时将 `game-server.exe`（Linux为 `game-server`）放在客户端可执行文件旁或资源目录。普通浏览器无法读取应用磁盘目录或启动本地进程，访问单人或教程会提示使用桌面客户端；多人模式仍可在浏览器使用。

`cargo test --manifest-path client/src-tauri/Cargo.toml --offline` 验证本地磁盘目录隔离、教程目录缓存和通关元数据读取；`client/tests/world-modes.cjs` 验证教程首次下载、本地导入、仅本地运行、缓存回退、重复进入与凭证隔离。服务端测试覆盖包导入、实际Python脚本喷射、进度持久化及联机模式拒绝本地包。

## 存档迁移

旧一体服的 `data/worlds` 和 `data/organism_instances` 格式兼容。停止旧一体进程后，可以让新游戏服的 `data_dir` 指向该旧 `data` 目录；中心仍使用原数据库和 `data/organism_models`。联机玩家ID必须来自原中心数据库，不能重建账号库后期待归属不变。

一个世界存档目录只允许一个运行进程写入；不要让旧一体服和新游戏服、或两台游戏服同时指向同一目录。多游戏服应各有独立的数据目录。中心签名私钥留在中心，迁移世界与实例文件时不要把它分发给游戏服。

## 验证

```powershell
cargo test -p server --offline
cargo build -p server --bins --offline
node server/tests/game-server-smoke.cjs
```

进程测试用临时配置分别启动中心验签模式和本地模式，联机模式用临时HTTP中心完成密钥注册和签名回执，不需要数据库，验证拒绝注册时不能启动、模型导入、Python AI、日志及重启恢复，并清理测试数据。Python默认使用 `python`，可用 `LIFE_PAPER_TEST_PYTHON` 指定解释器。

客户端 `client/tests/server-split.cjs` 验证单中心地址配置、大厅不访问游戏服、跨服世界路由、账号/票据隔离、模型包传递、并发续票、中继回退、写操作不重复提交、退出停止请求、空大厅及一体模式；需要 Vite 和 Playwright，沿用现有测试的 `WORLD_TEST_URL` 与 `PLAYWRIGHT_MODULE` 环境变量。`node --test client/tests/game-transport.mjs` 无需浏览器，验证地址校验、身份检查、读写回退、原始文件传输与取消请求。
