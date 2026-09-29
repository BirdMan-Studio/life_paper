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

- **Rust** — 客户端主要开发语言
- **eframe / egui** — 桌面客户端与 2D UI
- **Reqwest** — HTTP API 客户端
- **Tokio** — 后台异步请求
- **keyring** — 使用操作系统凭据库保存登录 token
- **TOML / Serde** — 客户端配置与本地化数据

## 项目结构

```text
life_paper/
├── client/
│   ├── i18n/             # zh_CN、en_US 本地化文本
│   └── src/
│       ├── api/          # 服务端 API 客户端
│       ├── components/   # 顶栏、侧栏、地图等公共组件
│       ├── pages/        # 登录、设置、世界等页面
│       ├── state/        # Session、token 和全局消息状态
│       ├── app.rs        # 客户端根组件与页面路由
│       └── main.rs
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

### 启动客户端

在项目根目录运行：

```powershell
cargo run -p client
```

客户端可以在设置页面修改服务器地址和界面语言。

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
- 客户端使用操作系统凭据库保存 token。
- 客户端启动时通过 `GET /account/me` 验证并恢复登录状态。
- 退出登录、注销账号、token 失效或切换服务器时会清除本地 token。

## 当前进度

当前版本仍处于早期开发阶段，已经完成：

- Cargo workspace 与客户端、服务端基础结构
- PostgreSQL 与 Redis 数据访问基础设施
- 用户注册、登录、退出登录和注销账号
- Redis session 创建、过期及全量撤销
- 当前账号信息查询接口
- 基于 eframe / egui 的桌面客户端
- 登录、注册、设置和世界概览页面
- `zh_CN`、`en_US` 界面本地化
- 系统凭据库 token 保存及自动登录恢复
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
