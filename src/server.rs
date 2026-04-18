use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::RwLock;
use crate::command::Command;
use crate::db::{Db, PubSubManager};
use crate::persistence::{self, PersistenceConfig};
use crate::protocol::Frame;


/// 全局变更计数器
pub static DIRTY_COUNT: AtomicU64 = AtomicU64::new(0);

pub async fn run_server() -> tokio::io::Result<()>{
    let config = PersistenceConfig::default(); // 初始化rdb配置
    let base_db = persistence::load_from_rdb(&config.rdb_path).await;
    let db: Db = Arc::new(RwLock::new(base_db)); // 生成数据库对象
    let pubsub = Arc::new(PubSubManager::new()); // 初始化广播管理器
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
        let pubsub_clone = Arc::clone(&pubsub); // 每次循环都克隆一个新的 Arc 指针
        tokio::spawn(async move {
            if let Err(e) = handle_stream(stream, db_clone, pubsub_clone).await {
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
async fn handle_stream(mut stream: TcpStream, db: Db, pubsub: Arc<PubSubManager>) -> tokio::io::Result<()> {
    loop {
        // 1. 读取并解析 Frame
        let frame = match read_frame(&mut stream).await? {
            Some(f) => f,
            None => break, 
        };

        // 2. 将 Frame 转为 Command
        let frames = match frame {
            Frame::Array(f) => f,
            _ => {
                stream.write_all(&Frame::Error("ERR protocol error".into()).to_bytes()).await?;
                continue;
            }
        };

        match Command::from_frames(frames) {
            Ok(cmd) => {
                // 3. 优雅分发：一句话搞定所有类型的命令
                if let Err(e) = cmd.apply(&db, &pubsub, &mut stream).await {
                    eprintln!("Command execution error: {:?}", e);
                    break;
                }
            },
            Err(e) => {
                stream.write_all(&Frame::Error(e).to_bytes()).await?;
            }
        }
    }
    Ok(())
}

/// 辅助函数：根据 RESP 首字节递归解析 Frame
pub async fn read_frame(stream: &mut TcpStream) -> tokio::io::Result<Option<Frame>> {
    let mut prefix = [0u8; 1];
    if stream.read_exact(&mut prefix).await.is_err() {
        return Ok(None);
    }

    match prefix[0] {
        b'*' => { // 解析数组
            let len = read_decimal(stream).await?;
            let mut frames = Vec::with_capacity(len as usize);
            for _ in 0..len {
                if let Some(f) = Box::pin(read_frame(stream)).await? {
                    frames.push(f);
                }
            }
            Ok(Some(Frame::Array(frames)))
        }
        b'$' => { // 解析 Bulk String
            let len = read_decimal(stream).await?;
            if len == -1 { return Ok(Some(Frame::Null)); }
            
            let mut data = vec![0u8; len as usize];
            stream.read_exact(&mut data).await?;
            stream.read_exact(&mut [0u8; 2]).await?; // 跳过 \r\n
            Ok(Some(Frame::Bulk(String::from_utf8_lossy(&data).to_string())))
        }
        b'+' => Ok(Some(Frame::Simple(read_line(stream).await?))),
        b':' => Ok(Some(Frame::Integer(read_decimal(stream).await?))),
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported RESP type")),
    }
}

/// 读取数字部分（直到 \r\n）
async fn read_decimal(stream: &mut TcpStream) -> tokio::io::Result<i64> {
    let line = read_line(stream).await?;
    line.parse().map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid number"))
}

/// 读取一行（直到 \r\n），返回字符串
async fn read_line(stream: &mut TcpStream) -> tokio::io::Result<String> {
    let mut line = Vec::new();
    loop {
        let b = stream.read_u8().await?;
        if b == b'\r' {
            let next = stream.read_u8().await?;
            if next == b'\n' { break; }
        }
        line.push(b);
    }
    Ok(String::from_utf8_lossy(&line).to_string())
}