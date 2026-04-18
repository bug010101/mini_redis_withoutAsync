use std::{ sync::Arc, time::Instant};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};
use std::collections::{HashMap, HashSet, VecDeque};


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")] // t: type; v: value
pub enum RedisValue {
    #[serde(rename = "s")]
    String(String),
    
    #[serde(rename = "l")]
    List(VecDeque<String>),
    
    #[serde(rename = "h")]
    Hash(HashMap<String, String>),
    
    #[serde(rename = "set")]
    Set(HashSet<String>),
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub value: RedisValue,
    // 使用Instant记录过期时刻，None表示永久有效
    #[serde(with = "timestamp_format")]
    pub expires_at: Option<Instant>,
}

// 由于Instant不能序列化，手动定义转换逻辑
mod timestamp_format {
    use std::time::{Instant, SystemTime, UNIX_EPOCH, Duration};
    use serde::{self, Deserialize, Deserializer, Serializer};

    // 序列化：Instant -> Unix Timestamp
    pub fn serialize<S>(instant: &Option<Instant>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let timestamp = instant.map(|inst| {
            let now_inst = Instant::now();
            let now_sys = SystemTime::now();
            if inst > now_inst {
                let duration = inst.duration_since(now_inst);
                now_sys.duration_since(UNIX_EPOCH).unwrap() + duration
            } else {
                now_sys.duration_since(UNIX_EPOCH).unwrap()
            }
        }).map(|d| d.as_secs());
        
        serde::Serialize::serialize(&timestamp, serializer)
    }

    // 反序列化：Unix Timestamp -> Instant
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Instant>, D::Error>
    where D: Deserializer<'de> {
        let timestamp: Option<u64> = Deserialize::deserialize(deserializer)?;
        Ok(timestamp.map(|secs| {
            let now_sys = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let now_inst = Instant::now();
            if secs > now_sys {
                now_inst + Duration::from_secs(secs - now_sys)
            } else {
                // 如果已过期，设为一个过去的时间点
                now_inst
            }
        }))
    }
}

impl Entry {
    pub fn new_string() -> Self {
        Self { value: RedisValue::String(String::new()), expires_at: None }
    }
}

/// 数据库基础类型名
pub type BaseDb = HashMap<String, Entry>;

/// 数据库类型名
pub type Db = Arc<RwLock<BaseDb>>;

// 发布订阅模式
// 频道中心：存的是各个频道的“发射塔” (Sender)
// 不需要序列化它，因为 Pub/Sub 消息不持久化
pub struct PubSubManager {
    channels: RwLock<HashMap<String, broadcast::Sender<String>>>,
}

impl PubSubManager {
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    // 获取某个频道的订阅器 (收音机)
    pub async fn subscribe(&self, channel: String) -> broadcast::Receiver<String> {
        let mut guard = self.channels.write().await;
        if let Some(sender) = guard.get(&channel) {
            sender.subscribe()
        } else {
            // 如果频道不存在，创建一个新的发射塔
            // 16 是缓冲区大小，表示如果订阅者处理太慢，最多积压16条消息
            let (tx, rx) = broadcast::channel(16);
            guard.insert(channel, tx);
            rx
        }
    }

    // 向某个频道发布消息
    pub async fn publish(&self, channel: &str, msg: String) -> usize {
        let guard = self.channels.read().await;
        if let Some(sender) = guard.get(channel) {
            // receiver_count() 返回当前有多少人在听
            sender.send(msg).unwrap_or(0)
        } else {
            0
        }
    }
}