use std::collections::HashMap;

use crate::db::{BaseDb, Db};

/// RDB持久化配置
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// RDB文件路径
    pub rdb_path: String,
    /// 自动保存间隔（秒）
    pub save_interval_secs: u64,
    /// 至少触发n次才执行保存操作
    pub save_min_changes: u64,
}

/// 使用Builder设计模式优化初始化操作
struct PersistenceConfigBuilder {
    /// RDB文件路径
    rdb_path: String,
    /// 自动保存间隔（秒）
    save_interval_secs: u64,
    /// 至少触发n次才执行保存操作
    save_min_changes: u64,
}

impl PersistenceConfigBuilder {
    fn new() -> Self {
        Self {
            rdb_path: "dump.rdb".to_string(),
            save_interval_secs: 60,
            save_min_changes: 1,
        }
    }

    fn rdb_path(mut self, path: &str) -> Self {
        self.rdb_path = path.to_string();
        self
    }
    
    fn save_interval_secs(mut self, secs: u64) -> Self {
        self.save_interval_secs = secs;
        self
    }

    fn save_min_changes(mut self, changes: u64) -> Self {
        self.save_min_changes = changes;
        self
    }
    #[must_use]
    fn build(self) -> PersistenceConfig {
        PersistenceConfig {
            rdb_path: self.rdb_path,
            save_interval_secs: self.save_interval_secs,
            save_min_changes: self.save_min_changes,
        }
    }
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        PersistenceConfig::builder().build()
    }
}

impl PersistenceConfig {
    pub fn builder() -> PersistenceConfigBuilder {
        PersistenceConfigBuilder::new()
    }
}
/// 从RDB文件加载数据
pub async fn load_from_rdb(path: &str) -> BaseDb {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            match serde_json::from_str::<BaseDb>(&content) {
                Ok(db) => {
                    println!("Loaded {} keys from {}", db.len(), path);
                    db
                },
                Err(e) => {
                    eprintln!("Failed to parse RDB file: {}, starting fresh", e);
                    HashMap::new()
                }
            }
        },
        Err(_) => {
            println!("No RDB file found, starting with empty database");
            HashMap::new()
        }
    }
}

/// 将数据保存到RDB文件
pub async fn save_to_rdb(db: &Db, path: &str) -> std::io::Result<()> {
    let guard = db.read().await;
    let json = serde_json::to_string_pretty(&*guard)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // 先写临时文件，再原子重命名，防止写到一半断电导致数据损坏
    let tmp_path = format!("{}.tmp", path);
    tokio::fs::write(&tmp_path, &json).await?;
    tokio::fs::rename(&tmp_path, &path).await?;
    Ok(())
}