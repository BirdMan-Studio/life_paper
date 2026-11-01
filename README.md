# 纸上谈兵（Life Paper）

**版本：** `0.0.0`

## 项目简介

**纸上谈兵（Life Paper）** 是一个用于锻炼 AI 强化学习能力的多人游戏项目。

玩家可以上传自己的 **AI 模型**与**生物**，并在服务器提供的生存环境中运行。

生物将使用玩家上传的模型进行决策，在有限的生存环境中不断进行生存与竞争，并以最终获得的**分数**作为评价依据。

项目的核心目标是通过游戏化的方式，为 AI 强化学习提供一个可持续运行、可量化比较的实验环境。

## 技术栈

### 服务端

- **Rust** — 服务端主要开发语言
- **Axum** — HTTP API 与路由
- **Tokio** — 异步运行时
- **SQLx** — PostgreSQL 数据访问
- **PostgreSQL** — 用户及游戏数据持久化
- **Redis** — 登录会话及高频数据存储
- **Argon2id** — 用户密码哈希
- **tracing** — 日志与运行状态追踪

### 客户端

- **Vue 3 / Vite** — 前端界面
- **Three.js** — 世界场景可视化
- **Tauri 2** — 桌面客户端容器

## 项目结构

```text
life_paper/
├── client/
│   ├── src/              # Vue 页面、组件、状态和 API 客户端
│   ├── src-tauri/        # Tauri 2 桌面壳
│   ├── package.json
│   └── vite.config.js
├── server/
│   ├── migrations/       # PostgreSQL 初始化迁移
│   └── src/
│       ├── api/          # HTTP API 路由
│       ├── db/           # PostgreSQL 与 Redis 数据访问
│       ├── domain/       # 领域数据结构
│       └── game/         # 游戏循环
├── common/
│   └── api.md            # v1 API 文档
├── Cargo.toml
└── README.md
```

## 快速开始

### 启动顺序

项目运行需要 PostgreSQL、Redis、Rust 和 Node.js。开发时建议按下面的顺序启动：

1. 启动 PostgreSQL 和 Redis。
2. 首次启动服务端，生成并填写配置文件。
3. 再次启动服务端，等待 API 监听 `8080` 端口。
4. 启动 Vue 开发客户端，或启动 Tauri 桌面客户端。

### 1. 首次配置并启动服务端

服务端使用 PostgreSQL 保存持久化数据，使用 Redis 保存登录 session。先确认两个服务已经运行，然后执行：

```powershell
cd server
cargo run
```

如果 `server/config.toml` 不存在，程序会生成配置模板并退出。编辑该文件，填写 PostgreSQL 和 Redis 配置后，再次启动：

```powershell
cargo run
```

首次初始化使用 `safe_mode = false`。数据库迁移和运行账号创建完成后，建议切换为 `safe_mode = true`，再重启服务端。

世界存档位于所加载的 `config.toml` 同级的 `data/worlds`，模型文件位于 `data/organism_models`。从仓库根目录或 `server` 目录启动都会找到 `server/config.toml` 并使用同一数据目录；也可用 `LIFE_PAPER_CONFIG` 指定配置文件路径。启动日志会显示配置和数据目录的绝对路径。重启后世界默认暂停，仍可在列表中查询、读取地层和生物，恢复后继续参与游戏循环。

实例代码保存在 `data/organism_instances`，入口为 `main.py` 中的 `main(game)`，可在世界的生物列表中打开编辑，世界模拟时每 tick 自动运行。Python 调用器默认关闭，开启配置、接口与执行限制见 [生物 AI 文档](common/ai.md)。

默认 API 地址为：

```text
http://127.0.0.1:8080/api/v1
```

### 2. 启动 Vue 客户端

另开一个终端，安装依赖并启动 Vite：

```powershell
cd client
npm install
npm run dev
```

浏览器访问 Vite 输出的地址，通常是 `http://127.0.0.1:1420/`。客户端默认连接上面的 API 地址。如果出现 `Failed to fetch`，请先检查服务端、PostgreSQL、Redis 是否正常运行。

### 3. 启动 Tauri 客户端

Tauri 运行前必须先启动服务端，并保持服务端终端运行：

```powershell
cd client
npm run desktop
```

该命令等同于 `npx tauri dev --no-dev-server`：先构建前端，再由桌面窗口直接加载内置资源，不启动 Vite 的 1420 端口或 Tauri 的静态资源服务器。修改前端后需重新执行命令，或运行 `npm run build` 更新资源；此模式不提供 Vite 热更新。客户端登录页右下角可以修改服务器地址。后端 API 服务仍需运行。

生产构建：

```powershell
npm run build
npx tauri build
```

客户端结构：

```text
client/src/
├── api/         # HTTP 请求、Bearer token、接口错误
├── stores/      # session 与世界/玩家/模型/组件状态
├── components/  # AppShell、Sidebar、TopBar、Three.js 场景
├── views/       # 登录、世界、生物、模型、设置页面
└── App.vue      # 路由与页面编排
```

业务数据来自服务端 API：登录后加载 `/account/me`、`/worlds`、`/player/data`、`/organism-models` 和 `/organism-components/unlocked`。服务端尚未提供生物实例列表、世界 tick、地图数据和活动日志接口；当前 `WorldScene` 只是静态视觉占位，不代表真实世界状态。

客户端默认连接 `http://127.0.0.1:8080/api/v1`，可通过登录页右下角的连接设置修改。

`client/src-tauri/Cargo.toml` 已声明独立 `[workspace]`，因此 Tauri 不会被根目录 Rust workspace 包含。

### 环境要求

- Rust stable
- PostgreSQL
- Redis

### 启动服务端

首次运行时，服务端会在当前目录生成 `config.toml` 并退出：

```powershell
cd server
cargo run
```

编辑生成的 `config.toml`，填写 PostgreSQL 和 Redis 配置后重新启动：

```powershell
cargo run
```

默认 API 地址：

```text
http://127.0.0.1:8080/api/v1
```

登录页右下角可以修改服务器地址；登录后的设置页面可以修改界面语言。

## 配置说明

### PostgreSQL safe mode

- `safe_mode = false`：初始化模式，使用高权限账号创建数据库、执行迁移并创建运行账号。
- `safe_mode = true`：运行模式，直接使用配置中的最小权限账号连接，不创建数据库或执行迁移。

初始化完成后，建议切换为 `safe_mode = true`。

服务端的 `config.toml` 已被 Git 忽略，不应提交真实数据库密码。

### 客户端配置

客户端会保存服务器地址和界面语言。默认服务器地址为：

```text
http://127.0.0.1:8080/api/v1
```

客户端当前支持 `zh_CN` 和 `en_US` 两种界面语言。

## 认证与安全

- 密码使用 Argon2id 和随机 salt 生成哈希。
- PostgreSQL 只保存 `password_hash`，不保存明文密码。
- 登录 session 存储在 Redis，并具有过期时间。
- 客户端不会保存用户密码。
- 客户端当前使用浏览器本地存储保存 token；不要在不受信任的设备上保留登录状态。
- 客户端启动时通过 `GET /account/me` 验证并恢复登录状态。
- 退出登录、注销账号、token 失效或切换服务器时会清除本地 token。

## 当前进度

当前版本仍处于早期开发阶段，已经完成：

- Cargo workspace 与客户端、服务端基础结构
- PostgreSQL 与 Redis 数据访问基础设施
- 用户注册、登录、退出登录和注销账号
- Redis session 创建、过期及全量撤销
- 当前账号信息查询接口
- 基于 Vue / Tauri 的桌面客户端
- 登录、注册、设置和世界概览页面
- `zh_CN`、`en_US` 界面本地化
- 本地 token 保存及自动登录恢复
- v1 API 文档基础结构

尚未完成：

- 正式游戏循环
- 世界状态同步
- 生物与模型管理
- 地图数据加载
- 自动化接口与客户端测试

## API 文档

当前 v1 API 设计与已实现接口见：

- [Life Paper API 文档](common/api.md)
