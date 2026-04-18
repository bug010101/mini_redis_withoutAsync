use std::{collections::{HashMap, HashSet, VecDeque}, sync::Arc, time::{Duration, Instant}};

use tokio::{io::AsyncWriteExt, net::TcpStream};

use crate::{db::{BaseDb, Db, Entry, PubSubManager, RedisValue}, protocol::Frame};

#[derive(Debug)]
pub enum Command {
    // RedisValue::String类型的枚举
    Set(String, String, Option<u64>), // 增加可选的秒数
    Get(String),
    Del(String),
    Exists(String),
    Incr(String),
    Decr(String),
    Incrby(String, String),
    Decrby(String, String),
    Append(String, String),
    Strlen(String),
    Getrange(String, String, String),

    // RedisValue::List类型的枚举
    LPUSH(String, Vec<String>), // 左边推入
    LPOP(String), // 左边弹出
    LRANGE(String, i64, i64), // 左数start到end

    // RedisValue::Hash类型的枚举
    HSet(String, String, String), // 设置字段: key, 字段， 值
    HGet(String, String), // 获取字段的值: key, 字段
    HGetAll(String), // 获取所有字段的值: key

    // RedisValue::Set类型的枚举
    SAdd(String, Vec<String>), // 添加元素: key, 元素集合
    SRem(String, Vec<String>), // 移除元素: key, 元素集合
    SMembers(String), // 获取元素: key
    // 通用类型的枚举
    Info,
    Exit,
    Ping,
    Expire(String, u64),

    // 发布订阅模式的命令
    Subscribe(String),
    Publish(String, String),
}


impl Command {
    /// 【新入口】分发命令：它是 handle_stream 直接调用的对象
    pub async fn apply(
        self, 
        db: &Db, 
        pubsub: &Arc<PubSubManager>, // 传入广播管理器
        stream: &mut TcpStream
    ) -> tokio::io::Result<()> {
        match self {
            // 优雅拦截：Subscribe 是“流式”的，直接接管 stream
            Command::Subscribe(channel) => {
                Self::handle_subscribe(stream, pubsub, channel).await
            }
            // 其余所有命令：都是“请求-响应”式的，走 execute
            other => {
                let (res_frame, dirty) = other.execute(db, pubsub).await;
                if dirty {
                    // 这里处理DIRTY_COUNT
                    crate::server::DIRTY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                stream.write_all(&res_frame.to_bytes()).await
            }
        }
    }

    async fn handle_subscribe(
        stream: &mut TcpStream, 
        pubsub: &Arc<PubSubManager>, 
        channel: String
    ) -> tokio::io::Result<()> {
        let mut rx = pubsub.subscribe(channel.clone()).await;
        
        // 1. 发送订阅成功确认
        let confirm = Frame::Array(vec![
            Frame::Bulk("subscribe".into()),
            Frame::Bulk(channel.clone()),
            Frame::Integer(1),
        ]);
        stream.write_all(&confirm.to_bytes()).await?;

        // 2. 持续推送消息
        loop {
            tokio::select! {
                msg_res = rx.recv() => {
                    if let Ok(msg) = msg_res {
                        let push = Frame::Array(vec![
                            Frame::Bulk("message".into()),
                            Frame::Bulk(channel.clone()),
                            Frame::Bulk(msg),
                        ]);
                        stream.write_all(&push.to_bytes()).await?;
                    }
                }
                // 必须检查连接是否断开，否则会造成内存泄露
                _ = stream.readable() => {
                    let mut buf = [0; 1];
                    if stream.try_read(&mut buf).is_ok() && buf[0] == 0 {
                        return Ok(()); 
                    }
                }
            }
        }
    }

    pub fn from_frames(frames: Vec<Frame>) -> Result<Self, String> {
        let mut parts = Vec::new();
        for frame in frames {
            match frame {
                // RESP 命令通常由 Bulk String 组成
                Frame::Bulk(s) | Frame::Simple(s) => parts.push(s),
                _ => return Err("Protocol error: expected bulk string in array".to_string()),
            }
        }
        if parts.is_empty() {
            return Err("Empty command".to_string());
        }
        // 核心：仅将第一个元素（命令名）转为小写
        let command_name = parts[0].to_lowercase();
        let args = &parts[1..];

        match (command_name.as_str(), args) {
            // String类型的枚举
            ("set", [key, value]) => Ok(Command::Set(key.clone(), value.clone(), None)),
            ("set", [key, value, ex, secs]) if ex.to_lowercase() == "ex" => {
                let s = secs.parse::<u64>().map_err(|_| "invalid expire time")?;
                Ok(Command::Set(key.clone(), value.clone(), Some(s)))
            },
            ("get", [key]) => Ok(Command::Get(key.clone())),
            ("del", [key]) => Ok(Command::Del(key.clone())),
            ("exists", [key]) => Ok(Command::Exists(key.clone())),
            ("incr", [key]) => Ok(Command::Incr(key.clone())),
            ("decr", [key]) => Ok(Command::Decr(key.clone())),
            ("incrby", [key, val]) => Ok(Command::Incrby(key.clone(), val.clone())),
            ("decrby", [key, val]) => Ok(Command::Decrby(key.clone(), val.clone())),
            ("append", [key, val]) => Ok(Command::Append(key.clone(), val.clone())),
            ("strlen", [key]) => Ok(Command::Strlen(key.clone())),
            ("getrange", [key, start, end]) => Ok(Command::Getrange(key.clone(), start.clone(), end.clone())),
            // List类型的枚举
            ("lpush", [key, rest @ ..]) => {
                let values = rest.iter().cloned().collect();
                Ok(Command::LPUSH(key.clone(), values))
            },
            ("lpop", [key]) => Ok(Command::LPOP(key.clone())),
            ("lrange", [key, start, end]) => {
                let start = start.parse::<i64>()
                    .map_err(|_| "ERR value is not an integer or out of range".to_string())?;
                let end = end.parse::<i64>()
                    .map_err(|_| "ERR value is not an integer or out of range".to_string())?;
                Ok(Command::LRANGE(key.clone(), start, end))
            },
            // Hash类型的枚举
            ("hset", [key, field, val]) => Ok(Command::HSet(key.clone(), field.clone(), val.clone())),
            ("hget", [key, field]) => Ok(Command::HGet(key.clone(), field.clone())),
            ("hgetall", [key]) => Ok(Command::HGetAll(key.clone())),
            // Set类型的枚举
            ("sadd", [key, rest @ ..]) => {
                let members = rest.iter().cloned().collect();
                Ok(Command::SAdd(key.clone(), members))
            },
            ("srem", [key, rest @ ..]) => {
                let members = rest.iter().cloned().collect();
                Ok(Command::SRem(key.clone(), members))
            },
            ("smembers", [key]) => Ok(Command::SMembers(key.clone())),
            // 通用类型的枚举
            ("expire", [key, secs]) => {
                let s = secs.parse::<u64>().map_err(|_| "invalid expire time")?;
                Ok(Command::Expire(key.clone(), s))
            },
            ("ping", []) => Ok(Command::Ping),
            ("info", []) => Ok(Command::Info),
            ("exit", []) => Ok(Command::Exit),
            // 发布订阅模式的枚举
            ("subscribe", [channel]) => Ok(Command::Subscribe(channel.clone())),
            ("publish", [channel, message]) => Ok(Command::Publish(channel.clone(), message.clone())),
            _ => Err(format!("unknown command or wrong arguments: {}", command_name)),
        }
    }


    pub async fn execute(&self, db: &Db, pubsub: &Arc<PubSubManager>) -> (Frame, bool) {
        match self {
            Command::Set(key, value, seconds) => {
                Self::write_operation(db, |db_write| {
                    let expires_at = seconds.map(|s| Instant::now() + Duration::from_secs(s));
                    db_write.insert(key.to_string(), Entry { value: RedisValue::String(value.to_string()), expires_at });
                    (Frame::Simple("OK".to_string()), true)
                }).await
            },
            Command::Get(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key);
                            (Frame::Null, true)
                        } else {
                            // 2. 类型检查：只有 String 类型才能被 GET
                            match &entry.value {
                                RedisValue::String(s) => {
                                    (Frame::Bulk(s.clone()), false)
                                }
                                // 3. 如果是 List, Hash 或 Set，返回 WRONGTYPE 错误
                                _ => (
                                    Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".to_string()),
                                    false
                                ),
                            }
                        }
                    } else {
                        (Frame::Null, false)
                    }
                }).await
            },
            Command::Del(key) => {
                Self::write_operation(db, |db_del| {
                    if db_del.remove(key).is_some() {
                        (Frame::Integer(1), true)
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Exists(key) => {
                Self::write_operation(db, |db_exists| {
                    if let Some(entry) = db_exists.get(key) {
                        if Self::is_expired(entry) {
                            db_exists.remove(key);
                            (Frame::Null, true)
                        } else {
                            (Frame::Integer(1), false)
                        }
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Incr(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                Command::execute_number(db, key, 1).await
            },
            Command::Decr(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                Command::execute_number(db, key, -1).await
            }
            Command::Incrby(key, increment) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                match increment.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, value).await
                    },
                    Err(_) => (Frame::Error("value is not an integer or out of range".to_string()), false),
                }
            },
            Command::Decrby(key, decrement ) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get(key) {
                        if Self::is_expired(entry) {
                            db_write.remove(key); 
                        }
                    }
                }).await;
                match decrement.parse::<i64>() {
                    Ok(value) => {
                        Command::execute_number(db, key, -value).await
                    },
                    Err(_) => (Frame::Error("value is not an integer or out of range".to_string()), false),
                }
            },
            Command::Append(key, value) => {
                Self::write_operation(db, |db_write| {
                    let s = db_write.entry(key.clone()).or_insert_with(Entry::new_string);
                    match &mut s.value {
                        RedisValue::String(s) => {
                            s.push_str(&value);
                            (Frame::Integer(s.len() as i64), true)
                        },
                        _ => (Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".into()), false),
                    }
                }).await
            },
            Command::Strlen(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(v) => match &v.value {
                            RedisValue::String(s) => (Frame::Integer(s.len() as i64), false),
                            _ => (Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".into()), false),
                        }
                        None => (Frame::Integer(0), false)
                    }
                }).await
            },
            Command::Getrange(key, start, end) => {
                Self::read_operation(db, |db_read| {
                    match (start.parse::<usize>(), end.parse::<usize>()) {
                        (Ok(start_idx), Ok(end_idx)) => {
                            match db_read.get(key) {
                                Some(v) => match &v.value {
                                    RedisValue::String(s) => {
                                        let bytes = s.as_bytes();
                                        let len = bytes.len();
                                        // 处理负数索引
                                        let start_idx = if start_idx > len { len } else { start_idx };
                                        let end_idx = if end_idx >= len { len - 1 } else { end_idx };
                                        if start_idx > end_idx {
                                            (Frame::Bulk("".to_string()), false)
                                        } else {
                                            let substring = String::from_utf8_lossy(&bytes[start_idx..=end_idx]);
                                            (Frame::Bulk(substring.to_string()), false)
                                        }
                                    },
                                    _ => (Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".into()), false),
                                },
                                None => (Frame::Null, false),
                            }
                        },
                        _ => (Frame::Error("value is not an integer or out of range".to_string()), false),
                    }
                }).await
            },

            Command::LPUSH(key, values) => {
                Self::write_operation(db, |db_write| {
                    // 如果不存在，创建一个新的 RedisValue::List
                    let entry = db_write.entry(key.clone()).or_insert_with(|| Entry {
                        value: RedisValue::List(VecDeque::new()),
                        expires_at: None,
                    });

                    match &mut entry.value {
                        RedisValue::List(list) => {
                            for v in values {
                                list.push_front(v.to_string()); // 从左侧依次推入
                            }
                            (Frame::Integer(list.len() as i64), true)
                        }
                        // 如果这个 key 已经存了 String，就报错
                        _ => (Frame::Error("WRONGTYPE ...".into()), false),
                    }
                }).await
            },
            Command::LPOP(key) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get_mut(key) {
                        match &mut entry.value {
                            RedisValue::List(list) => {
                                match list.pop_front() {
                                    Some(val) => (Frame::Bulk(val), true),
                                    None => (Frame::Null, false), // 列表空了
                                }
                            }
                            _ => (Frame::Error("WRONGTYPE ...".into()), false),
                        }
                    } else {
                        (Frame::Null, false) // Key 不存在
                    }
                }).await
            },
            Command::LRANGE(key, start, end) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(entry) => match &entry.value {
                            RedisValue::List(list) => {
                                let len = list.len() as i64;
                                if len == 0 { return (Frame::Array(vec![]), false); }

                                // --- 核心逻辑：处理负数和越界 ---
                                let mut s = if *start < 0 { len + start } else { *start };
                                let mut e = if *end < 0 { len + end } else { *end };

                                // 规范化范围
                                if s < 0 { s = 0; }
                                if e >= len { e = len - 1; }

                                if s > e || s >= len {
                                    (Frame::Array(vec![]), false)
                                } else {
                                    // 提取范围内的元素
                                    let frames = list.iter()
                                        .skip(s as usize)
                                        .take((e - s + 1) as usize)
                                        .map(|v| Frame::Bulk(v.clone()))
                                        .collect();
                                    (Frame::Array(frames), false)
                                }
                            }
                            _ => (Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".into()), false),
                        },
                        None => (Frame::Array(vec![]), false),
                    }
                }).await
            },

            Command::HSet(key, field, value) => {
                Self::write_operation(db, |db_write| {
                    let entry = db_write.entry(key.clone()).or_insert_with(|| Entry {
                        value: RedisValue::Hash(HashMap::new()),
                        expires_at: None,
                    });

                    match &mut entry.value {
                        RedisValue::Hash(map) => {
                            map.insert(field.clone(), value.clone());
                            (Frame::Integer(1), true) // 返回 1 表示设置成功
                        }
                        _ => (Frame::Error("WRONGTYPE ...".into()), false),
                    }
                }).await
            },
            Command::HGet(key, field) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(entry) => match &entry.value {
                            RedisValue::Hash(map) => {
                                match map.get(field) {
                                    Some(val) => (Frame::Bulk(val.clone()), false),
                                    None => (Frame::Null, false),
                                }
                            }
                            _ => (Frame::Error("WRONGTYPE ...".into()), false),
                        },
                        None => (Frame::Null, false),
                    }
                }).await
            },
            Command::HGetAll(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(entry) => match &entry.value {
                            RedisValue::Hash(map) => {
                                let mut frames = Vec::new();
                                for (f, v) in map {
                                    frames.push(Frame::Bulk(f.clone()));
                                    frames.push(Frame::Bulk(v.clone()));
                                }
                                (Frame::Array(frames), false)
                            }
                            _ => (Frame::Error("WRONGTYPE ...".into()), false),
                        },
                        None => (Frame::Array(vec![]), false),
                    }
                }).await
            },

            Command::SAdd(key, members) => {
                Self::write_operation(db, |db_write| {
                    let entry = db_write.entry(key.clone()).or_insert_with(|| Entry {
                        value: RedisValue::Set(HashSet::new()),
                        expires_at: None,
                    });
                    match &mut entry.value {
                        RedisValue::Set(set) => {
                            let mut added = 0;
                            for m in members {
                                if set.insert(m.clone()) {
                                    added += 1;
                                }
                            }
                            (Frame::Integer(added), added > 0) // 只有真正新增了才算 dirty
                        }
                        _ => (Frame::Error("WRONGTYPE ...".into()), false),
                    }
                }).await
            },
            Command::SRem(key, members) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get_mut(key) {
                        match &mut entry.value {
                            RedisValue::Set(set) => {
                                let mut removed = 0;
                                for m in members {
                                    if set.remove(m) {
                                        removed += 1;
                                    }
                                }
                                (Frame::Integer(removed), removed > 0)
                            }
                            _ => (Frame::Error("WRONGTYPE ...".into()), false),
                        }
                    } else {
                        (Frame::Integer(0), false) // Key 不存在，移除 0 个
                    }
                }).await
            },
            Command::SMembers(key) => {
                Self::read_operation(db, |db_read| {
                    match db_read.get(key) {
                        Some(entry) => match &entry.value {
                            RedisValue::Set(set) => {
                                let frames = set.iter()
                                    .map(|m| Frame::Bulk(m.clone()))
                                    .collect();
                                (Frame::Array(frames), false)
                            }
                            _ => (Frame::Error("WRONGTYPE ...".into()), false),
                        },
                        None => (Frame::Array(vec![]), false),
                    }
                }).await
            },

            Command::Info => {
                // 不用闭包，用守卫操作
                let read_guard = db.read().await;
                // 统计带有过期时间且尚未过期的 key 数量
                let expires_count = read_guard.values()
                    .filter(|e| Self::is_expired(e))
                    .count();

                let info_content = format!(
                    "# Server\r\nversion:0.1.0\r\nos:{}\r\n# Keyspace\r\ndb0:keys={},expires={}\r\n",
                    std::env::consts::OS, 
                    read_guard.len(),
                    expires_count
                );
                (Frame::Bulk(info_content), false)
            },
            Command::Ping => {
                (Frame::Simple("PONG".to_string()), false)
            }
            Command::Exit => {
                (Frame::Simple("OK".to_string()), false)
            },
            Command::Expire(key, seconds) => {
                Self::write_operation(db, |db_write| {
                    if let Some(entry) = db_write.get_mut(key) {
                        entry.expires_at = Some(Instant::now() + Duration::from_secs(*seconds));
                        (Frame::Integer(1), true)
                    } else {
                        (Frame::Integer(0), false)
                    }
                }).await
            },

            Command::Publish(channel, message) => {
                let count = pubsub.publish(&channel, message.to_string()).await;
                (Frame::Integer(count as i64), false)
            },
            // 告诉编译器：Subscribe 永远不会走到这里
            Command::Subscribe(_) => unreachable!("Subscribe should be handled by Command::apply"),
        }
    }

    async fn execute_number(db: &Db, key: &String, increment: i64) -> (Frame, bool) {
        Self::write_operation(db, |db_map| {
            let entry = db_map.entry(key.to_string()).or_insert_with(|| 
                Entry { value: RedisValue::String("0".to_string()), expires_at: None, }
            );

            match &mut entry.value {
                RedisValue::String(ref mut s) => {
                    let current = s.parse::<i64>().unwrap_or(0);
                    let next_value = current + increment;
                    *s = next_value.to_string();
                    (Frame::Integer(next_value), true)
                },
                _ => (
                    Frame::Error("WRONGTYPE Operation against a key holding the wrong kind of value".into()),
                    false
                ),
            }
        }).await
    }

    async fn read_operation<F, T>(db: &Db, f: F) -> T
    where 
        F: FnOnce(&BaseDb) -> T
    {
        let guard = db.read().await;
        f(&*guard)
    }

    async fn write_operation<F, T>(db: &Db, f: F) -> T
    where 
        F: FnOnce(&mut BaseDb) -> T
    {
        let mut guard = db.write().await;
        f(&mut *guard)
    }

    // 判断是否过期
    fn is_expired(entry: &Entry) -> bool {
        entry.expires_at.map_or(false, |expired| expired <= Instant::now())
    }
}
