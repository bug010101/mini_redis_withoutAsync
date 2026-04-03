# Mini Redis

一个用 Rust 实现的简单 Redis 服务器（异步版本）

## 功能
- set key value - 设置键值对
- get key - 获取值
- del key - 删除值
- exists key - 是否存在值
- incr key - key(i64) + 1
- decr key - key(i64) - 1
- incrby key value - key(i64) + value
- decrby key value - key(i64) - value
- append key value - db[key]后面连接value
- strlen key - 获取长度
- getrange key 1 5 - 获取区间长度
- info - 获取当前的状态信息
## 编译

```bash
cargo build --release
```

## 编译运行
```bash
cargo run
```

## 一键测试
```
cargo test
```

## 测试
```
> nc localhost:6379 
> set key 1 
> get key 
> del key 
> exists key 
> incr key 
> decr key 
> incrby key 5 
> decrby key 5 
> append key hello 
> strlen key 
> getrange key 1 5 
> info 
```