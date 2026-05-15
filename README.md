# Jin-proxy — DeepSeek API Proxy (Rust)

将 OpenAI Responses API / Anthropic Messages API 翻译为 DeepSeek Chat Completions API，支持 Codex CLI 和 Claude Code。

## 快速开始

**要求**：无需安装任何运行时，单文件可执行。

```powershell
# 必须配置
$env:DEEPSEEK_KEY="sk-your-deepseek-api-key"

# 可选配置（有默认值）
$env:PROXY_PORT="8080"
$env:ADMIN_PORT="8090"

# 运行
.\jin-proxy.exe
```

启动后访问 `http://127.0.0.1:8090` 进入管理面板，所有参数即时调整即时生效。

## 客户端配置

代理启动后，各客户端的连接方式：

```powershell
# Codex CLI
$env:OPENAI_BASE_URL="http://127.0.0.1:8080"
$env:OPENAI_API_KEY="你的DeepSeek-Key"
codex

# Claude Code
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
$env:ANTHROPIC_API_KEY="你的DeepSeek-Key"
claude
```

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DEEPSEEK_KEY` | — | **必填**，DeepSeek API Key |
| `DEEPSEEK_BASE` | `https://api.deepseek.com` | 上游 API 地址 |
| `DEFAULT_MODEL` | `deepseek-v4-pro` | 默认模型 |
| `PROXY_PORT` | `8080` | HTTP/WS 代理端口 |
| `TLS_PORT` | `8444` | 直接 TLS 端口 |
| `CONNECT_PORT` | `8443` | CONNECT 隧道端口 |
| `ADMIN_PORT` | `8090` | Web 管理面板端口 |
| `DEFAULT_REASONING_EFFORT` | — | 推理强度 (min/low/medium/high/max) |
| `MAX_POSITION_EMBEDDINGS` | `1000000` | 最大上下文长度 |
| `REASONING_CACHE_MAX` | `10` | 每会话最多缓存的推理条数 |
| `REASONING_CACHE_TTL` | `600` | 推理缓存有效期（秒） |

## 架构

```
                         ┌──────────────────────────────┐
                         │         Jin-proxy (Rust)       │
                         │                                │
  Codex CLI ─────────────▶ 8444 (TLS 直连)               │
  (OPENAI_BASE_URL)      │                                │
                         │ Claude Code ─────▶ 8080 (HTTP) │
                         │ (ANTHROPIC_BASE_URL)           │
                         │                                │
                         │ 8090 (Admin UI) ── Web 管理界面 │
                         └──────────────┬─────────────────┘
                                        │
                                        ▼
                               api.deepseek.com
                            (Chat Completions API)
```

**三个服务端口**：

| 端口 | 协议 | 用途 |
|------|------|------|
| 8080 | HTTP | Responses API 翻译 + SSE 流式 |
| 8444 | HTTPS | 直接 TLS 终止 |
| 8090 | HTTP | Web 管理面板 |

**请求处理流程**：

```
Responses API 请求
  → 模型名映射（gpt-5.5 → deepseek-v4-pro）
  → 首轮：网页预取 + tool_use 提示注入（后续轮次跳过，保护 prompt cache）
  → 推理缓存注入（上轮 thinking → assistant 消息，thinking 关闭时跳过）
  → 转换为 Chat Completions 格式
  → 发送到 api.deepseek.com
  → SSE 翻译回 Responses 事件流
  → 缓存本轮推理内容
```

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/v1/messages`、`/messages` | Anthropic Messages→Chat 翻译（Claude Code） |
| GET | `/v1/models/claude` | Claude Code 模型列表 |
| POST | `/v1/chat/completions`、`/chat/completions` | Chat Completions 透传 |
| POST | `/v1/responses`、`/responses` | Responses→Chat 翻译 |
| POST | `/v1/responses/compact` | 对话压缩 |
| POST | `/backend-api/codex/responses` | Codex 专用 Responses 路由 |
| GET | `/v1/models`、`/models` | 模型列表 |
| GET | `/health` | 健康检查 |
| GET | `/backend-api/codex/models` | Codex 模型目录 |
| POST | `/backend-api/codex/analytics-events/events` | Codex 遥测桩 |
| ANY | `/backend-api/{path}` | Codex 后端兜底 |

## 管理界面

浏览器打开 `http://127.0.0.1:8090`：

- **上游连接**：API Key、Base URL、默认模型
- **模型映射**：OpenAI 模型名 → DeepSeek 模型名
- **生成参数**：推理强度、上下文窗口、最大输出
- **网页抓取**：最大 URL 数、超时、响应体上限
- **推理缓存**：开关 + TTL
- **实时统计**：运行时间、请求数、活跃流、错误率、缓存命中率
- **终端环境变量**：一键复制 Codex CLI / Claude Code 环境变量

所有配置保存即时生效，无需重启。

## 关键实现细节

### 多工具调用合并
DeepSeek 要求同一轮的所有 tool_calls 必须在一个 assistant 消息中。代理自动将 Codex 发送的多个独立 function_call 合并为一条符合要求的消息。

### 网页预取
发现用户消息中的 URL 后，代理预先抓取网页内容注入对话上下文，模型直接使用内容而无需发起 web_fetch 工具调用。**仅首轮注入**，避免后续轮次修改消息破坏 DeepSeek prompt cache。

### 推理缓存
DeepSeek 思考模式要求所有 assistant 消息都携带 `reasoning_content`。代理缓存每轮对话的推理内容（本地文件缓存），下一轮注入到历史 assistant 消息中。

- Codex 与 Claude 的推理缓存完全隔离
- 默认关闭 thinking，可在管理面板开启
- 缓存注入仅在 thinking 启用时生效

### Prompt Cache 保护
- `reasoning_content` 仅在有缓存数据时才注入，不填充空字符串
- `tool_use` 强制提示、`web_fetch` 预取仅首轮注入
- Claude 通道在 thinking 关闭时完全跳过缓存注入

### TLS 证书管理
首次启动自动生成 CA+Server 证书链（CA: CN=JinDX-CA，Server 由 CA 签发），SAN 覆盖 localhost、api.openai.com 等 7 个域名。有效期 5 年。使用 rcgen 库（跨平台，无需 openssl）。CA 证书单独输出为 `ca.pem` 方便用户导入系统信任。

### Session 隔离
Codex 和 Claude 分别使用 `"codex"` 和 `"claude"` 作为缓存目录，Session 缓存完全隔离。Session ID 使用 SHA256 哈希生成，降低碰撞概率。

## 构建

```bash
cargo build --release
# 输出：target/release/jin-proxy.exe (~24MB)
```

## 常用命令

```powershell
# 测试代理
curl.exe -s http://127.0.0.1:8080/health

# 调整参数
curl.exe -X POST http://127.0.0.1:8090/config -H "Content-Type: application/json" -d '{"temperature":0.7,"reasoning_effort":"high"}'
```
