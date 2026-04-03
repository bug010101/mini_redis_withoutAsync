use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 数据库基础类型名
pub type BaseDb = HashMap<String, String>;

/// 数据库类型名
pub type Db = Arc<RwLock<BaseDb>>;
