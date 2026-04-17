use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::RwLock;
use crate::command::Command;
use crate::db::Db;
use crate::persistence::{self, PersistenceConfig};


/// 全局变更计数器
static DIRTY_COUNT: AtomicU64 = AtomicU64::new(0);

pub async fn run_server() -> tokio::io::Result<()>{
    let config = PersistenceConfig::default(); // 初始化rdb配置
    let base_db = persistence::load_from_rdb(&config.rdb_path).await;
    let db: Db = Arc::new(RwLock::new(base_db)); // 生成数据库对象
    
    // 启动定时持久化任务
    let db_clone = Arc::clone(&db);
    let config_clone = config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(config_clone.save_interval_secs));
        loop {
            interval.tick().await;
            let dirty = DIRTY_COUNT.load(Ordering::Relaxed);
            if dirty >= config_clone.save_min_changes {
                match persistence::save_to_rdb(&db_clone, &config_clone.rdb_path).await {
                    Ok(_) => DIRTY_COUNT.store(0, Ordering::Relaxed),
                    Err(e) => eprintln!("RDB save failed: {}", e),
                }
            }
        }
    });

    // 启动过期清理任务
    let db_cleanup = Arc::clone(&db);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            // 获取数据库操作的守卫
            let mut db_write = db_cleanup.write().await; 
            let now = Instant::now();
            let before_len = db_write.len();
            // 保留未过期的数据
            db_write.retain(|_, entry| {
                entry.expires_at.map_or(true, |at| at > now)
            });
            let deleted = before_len - db_write.len();
            if deleted > 0 {
                // 如果删除了过期key，增加全局写操作计数，触发后续RDB保存
                DIRTY_COUNT.fetch_add(deleted as u64, Ordering::Relaxed);
                println!("Deleted {} expired keys", deleted);
            }
        }
    });

    // 监听关闭信号，优雅退出
    let db_shutdown = Arc::clone(&db);
    let rdb_path = config.rdb_path.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down, saving final snapshot...");
        persistence::save_to_rdb(&db_shutdown, &rdb_path).await.ok();
        std::process::exit(0);
    });
    // TCP连接
    let listener = TcpListener::bind("0.0.0.0:6379").await?; // 绑定listerner
    println!("Server running on 0.0.0.0:6379");

    loop {
        let (stream, socket_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Accept failed: {}", e);
                continue; // 继续接受下一个连接
            }
        };
        println!("socket_addr in on {:?}", socket_addr);
        let db_clone = Arc::clone(&db);
        
        tokio::spawn(async move {
            if let Err(e) = handle_stream(stream, db_clone).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}

/// 给定TcpStream，数据库
/// 使用BufReader处理连接
/// 1、根据stream连接获取reader对象
/// 2、开启循环，读取每个命令直到没有任何命令输入为止
/// 3、将输入的命令进行处理，判断，解析，执行
/// 4、将处理的命令传回连接返回给客户端
async fn handle_stream(mut stream: TcpStream, db: Db) -> tokio::io::Result<()>{
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimed_cmd = line.trim();
        if trimed_cmd.is_empty() {
            continue;
        }
        let command = Command::from_str(trimed_cmd);
        let is_exit = matches!(command, Ok(Command::Exit));
        let response = match command{
            Ok(cmd) => {
                let (resp, dirty) = cmd.execute(&db).await;
                if dirty {
                    DIRTY_COUNT.fetch_add(1, Ordering::Relaxed);
                }
                resp
            },
            Err(e) => format!("-ERR {}\r\n", e),
        };
        // 异步写回
        writer.write_all(response.as_bytes()).await?;
        if is_exit {
            println!("客户端请求退出");
            break;
        }
    }
    Ok(())
}
