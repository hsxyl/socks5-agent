# EdgeProxy-Rust

EdgeProxy-Rust 是一个专为边缘设备（如机顶盒、智能路由器等）设计的高性能、低内存占用的**住宅 IP 代理 (Residential Proxy)** 系统。它允许边缘设备在 NAT（路由器）之后主动建立反向隧道，将设备转化为云端可控的 SOCKS5 代理节点。同时内置完善的**商业化卡密计费系统**与 **React 可视化管理面板**。

## 🌟 核心特性

- **极致轻量**：客户端完全基于 Rust 编写，无运行时开销，内存占用极低（仅需几 MB 内存），完美适配各类资源受限的边缘设备。
- **真·多路复用**：底层采用 `Yamux` 协议在单条 TCP 长连接上虚拟出成千上万条独立流，彻底解决传统代理模型中单连接并发阻塞问题，有效穿透 NAT 和严格防火墙。
- **协议解耦**：客户端仅负责最底层的 "盲转发 (Blind Forwarding)"。复杂的 SOCKS5/HTTP 协议握手、地址解析均在云端服务器完成，最大限度减轻了机顶盒的算力压力。
- **卡密计费与 SOCKS5 鉴权**：内置 SQLite 数据库，用户连接 SOCKS5 代理时必须使用分配的卡密作为账密进行 Auth 鉴权。系统支持精确到 Byte 级别的双向流量统计及扣费功能。
- **第三方代理池融合**：当边缘节点全部离线时，系统会自动在自建数据库中抽取第三方备用 IP 代理进行自动托底路由转发。
- **可视化实时监控面板**：配套基于 React + Vite 开发的现代 Admin Web UI，帮助你实时掌握机顶盒在线状态、活跃设备 IP 以及连续服务时长。

---

## 🏗 架构总览

系统分为四个核心组件，被组织在一个 Cargo Workspace 中：

- **`common` (核心协议库)**: 定义了控制命令结构 (`ControlMessage`)、代理转发请求 (`ProxyRequest`) 以及用于隧道通信的异步二进制协议扩展。
- **`server` (云端控制台)**: 部署在具有公网 IP 的服务器上。
  - **边缘接入端**: 监听 `8080` 端口，等待 `Client` 连接，接受注册与心跳，维护存活设备的并发 Session。
  - **用户代理端**: 支持双协议栈。监听 `1080` 端口提供 SOCKS5 代理，监听 `8081` 端口提供 HTTP 代理。负责卡密验证、设备定向路由与流量扣费。
  - **Web API 端**: 监听 `3000` 端口，提供 `axum` 驱动的 RESTful API 供前端面板拉取实时数据。
- **`client` (边缘节点)**: 部署在机顶盒或边缘网关上。启动后主动向 `Server` 发起长连接并注册身份，随后在后台执行代理转流任务。
- **`admin-web` (React 面板)**: 提供直观的前端交互界面，查看各边缘设备的在线情况与历史心跳记录。

---

## 🚀 快速开始

### 1. 环境依赖
- [Rust 1.70+](https://rustup.rs/) (用于开发和构建)
- [Node.js 18+](https://nodejs.org/) (用于编译 React 管理界面)

### 2. 编译项目
```bash
# 构建核心服务端与客户端
cargo build --release
```

### 3. 运行服务端 (Server)
服务端运行在具有公网 IP 的机器上，启动时会自动在当前目录生成或加载 `data.db` 数据库文件。
```bash
# 默认配置：在 0.0.0.0:8080 监听边缘节点，0.0.0.0:1080 (SOCKS5), 0.0.0.0:8081 (HTTP) 提供代理，0.0.0.0:3000 提供前端接口
cargo run --bin server --release

# 【进阶】生成一张含有 10 GB 流量 (10*1024*1024*1024 bytes) 的卡密
cargo run --bin server --release -- add-card --balance 10737418240

# 【进阶】录入一条你购买的第三方备用 SOCKS5 代理资源
cargo run --bin server --release -- add-proxy --address "8.8.8.8:1080" --ptype "SOCKS5"
```

### 4. 启动可视化管理面板 (React Admin UI)
管理面板允许你直观地监控机顶盒资源：
```bash
cd admin-web
npm install

# 启动本地开发服务 (默认监听 5173 端口)
npm run dev
# 或进行生产环境编译
# npm run build
```
然后在浏览器中打开 `http://localhost:5173` 即可看到实时看板。

### 5. 运行客户端 (Client 边缘节点)
客户端部署在边缘设备上。需指定服务端的真实 IP 或域名：
```bash
# 连接远端服务器，并显式分配设备 ID 和心跳间隔 (秒)
cargo run --bin client --release -- --server-addr 192.168.1.100:8080 --device-id "android-tv-001" --heartbeat-interval 15

# 或使用环境变量启动
SERVER_ADDR="192.168.1.100:8080" DEVICE_ID="android-tv-001" HEARTBEAT_INTERVAL=15 cargo run --bin client --release
```

### 6. 使用代理网络与定向路由测试
连接成功后，普通用户只需将自己购买的 **卡密** 作为用户名填入 SOCKS5 或 HTTP 代理请求中。
此外，你还可以通过 **密码 (Password)** 字段动态指定目标设备：如果留空，服务端会**随机**进行负载均衡；如果填入具体的 `Device ID`，服务端则会强制将流量路由至该边缘设备。

#### SOCKS5 代理测试 (端口 1080)
```bash
# 格式：curl -x socks5h://[卡密]:[可选目标设备ID]@[服务器公网IP]:1080 http://ifconfig.me
# 示例：强制让流量走刚才启动的 "android-tv-001" 设备
curl -x socks5h://[你生成的卡密]:android-tv-001@[服务器公网IP]:1080 http://ifconfig.me
```

#### HTTP 代理测试 (端口 8081)
```bash
# 格式：curl -x http://[卡密]:[可选目标设备ID]@[服务器公网IP]:8081 http://ifconfig.me
# 示例：不指定设备，使用随机负载均衡策略
curl -x http://[你生成的卡密]:@[服务器公网IP]:8081 http://ifconfig.me
```

---

## 🛠 交叉编译指南 (机顶盒部署方案)

由于边缘设备（如机顶盒、路由器）多采用 ARM 或 MIPS 架构且缺乏运行环境，强烈推荐使用 [`cross`](https://github.com/cross-rs/cross) 工具进行交叉编译。它可输出链接 `musl libc` 的纯静态二进制文件，真正做到**零依赖**拖拽运行。

```bash
# 1. 安装 cross 交叉编译工具
cargo install cross --git https://github.com/cross-rs/cross

# 2. 针对 ARM64 (AArch64) 机顶盒编译
cross build --bin client --target aarch64-unknown-linux-musl --release

# 3. 针对 ARM32 (armv7) 机顶盒编译
cross build --bin client --target armv7-unknown-linux-musleabihf --release

# 编译完成后，将 target/[架构]/release/client 文件上传至机顶盒直接运行即可！
```

---

## 📂 核心代码结构

```text
.
├── Cargo.toml                # Workspace 根配置
├── common                    # 核心抽象和协议库
│   └── src/protocol.rs       # 隧道控制帧与代理指令协议设计
├── server                    # 云端中继与调度服务端
│   ├── src/main.rs           # 服务端入口 (含命令行与 API 路由)
│   ├── src/db.rs             # SQLite 数据库模型与 ORM (卡密/代理/设备状态)
│   ├── src/client_handler.rs # 设备长连接、Yamux 后台死循环驱动及鉴权
│   ├── src/session.rs        # SessionManager (设备状态并发管理池与随机负载均衡)
│   ├── src/socks5.rs         # SOCKS5 Auth(0x02)、计费与设备定向路由
│   └── src/http_proxy.rs     # HTTP CONNECT Proxy (基于 Base64 Auth)、设备定向路由
├── client                    # 部署在机顶盒的轻量级边缘代理客端
│   ├── src/engine.rs         # 核心引擎：多路复用信道维护、反向连接透传
│   └── src/main.rs           # 配置项挂载
└── admin-web                 # React 可视化大屏 / 管理面板
    ├── src/App.tsx           # 面板数据请求与 UI 表单渲染
    └── src/App.css           # 纯净版组件样式
```
