***
# Mini-Redis-Rust

这是一个使用 Rust 语言和 `tokio` 异步运行时实现的轻量级 Redis 服务器。它完整实现了 Redis 序列化协议 (RESP)，并支持高性能的并发命令处理、数据持久化以及实时的发布/订阅功能。

## 🚀 技术亮点

* **全异步架构**：基于 `Tokio` 运行时，利用 `async/await` 实现高并发连接处理。
* **零拷贝思想**：在协议解析过程中尽可能减少内存拷贝，提升吞吐量。
* **线程安全**：使用 `Arc<RwLock<T>>` 确保多线程环境下内存数据库的安全访问。
* **发布/订阅机制**：利用 `tokio::sync::broadcast` 实现高性能的消息分发，并使用 `tokio::select!` 优化了连接的健壮性。
* **二进制安全**：RESP 协议实现支持任何二进制数据（如中文、图片字节流等）。

## 🛠 已实现功能

### 1. 基础键值操作 (Strings)
* `SET key value [EX seconds]`：设置键值对，支持可选的过期时间。
* `GET key`：获取指定键的值。
* `DEL key`：删除键。
* `EXISTS key`：检查键是否存在。
* `INCR / DECR`：对数值进行自增/自减。

### 2. 高级数据结构
* **List (列表)**：支持 `LPUSH`、`LPOP`、`LRANGE`。
* **Hash (哈希)**：支持 `HSET`、`HGET`、`HGETALL`。
* **Set (集合)**：支持 `SADD`、`SREM`、`SMEMBERS`。

### 3. 发布/订阅 (Pub/Sub)
* `SUBSCRIBE channel`：订阅频道，支持实时消息推送。
* `PUBLISH channel message`：向指定频道发布消息。

### 4. 系统功能
* **持久化**：支持 RDB 快照功能，定时将内存数据以 JSON 格式保存到磁盘。
* **生存时间 (TTL)**：支持 Key 的自动过期清理。
* **PING**：心跳检测。

## 📦 安装与运行

### 环境要求
* Rust 1.70.0 或更高版本
* Cargo

### 编译与启动
1. 克隆项目：
   ```bash
   git clone https://github.com/bug010101/mini_redis_withoutAsync.git
   cd mini_redis
   ```
2. 运行服务器：
   ```bash
   cargo run
   ```
   默认服务器将监听 `127.0.0.1:6379`。

## 📖 使用教程

你可以使用官方的 `redis-cli` 或者 `nc` (Netcat) 与服务器通信。

### 使用 redis-cli 测试（推荐）

**1. 基础存取：**
```bash
redis-cli SET name "Rust"
redis-cli GET name
```

**2. 测试发布订阅：**
* **窗口 A (订阅者):**
    ```bash
    redis-cli --raw SUBSCRIBE my_chat
    ```
* **窗口 B (发布者):**
    ```bash
    redis-cli PUBLISH my_chat "你好，Redis！"
    ```

### 使用 Netcat 手动发送 RESP 协议
由于项目严格遵循 RESP 协议，你可以手动发送原始字节：
```bash
printf "*2\r\n$4\r\nINFO\r\n" | nc localhost 6379
```

## 📂 项目结构

```text
src/
├── main.rs          # 实例入口
├── lib.rs           # 使用的crate
├── server.rs        # TCP 监听、连接调度与协议处理
├── db.rs            # 内存数据库模型与 Pub/Sub 管理器
├── protocol.rs      # RESP 协议帧 (Frame) 的编解码逻辑
├── command.rs       # 各类 Redis 命令的具体业务执行
└── persistence.rs   # RDB 持久化逻辑 (JSON 序列化)
```

## 🛡 网络健壮性说明
本项目在 `handle_subscribe` 中使用了 `tokio::select!` 宏，能够同时监听内部频道消息和客户端指令。当客户端意外断开或发送退出信号时，服务器能立即回收资源，避免了内存泄露和僵尸连接问题。

## ⚖ 许可证
Apache License Version 2.0
***