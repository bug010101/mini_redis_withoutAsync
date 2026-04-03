use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use crate::command::Command;
use crate::db::Db;

pub async fn run_server() -> tokio::io::Result<()>{
    let db: Db = Arc::new(RwLock::new(HashMap::new())); // 生成数据库对象
    let listener = TcpListener::bind("0.0.0.0:6379").await?; // 绑定listerner
    println!("Server running on 0.0.0.0:6379");

    loop {
        let (stream, socket_addr) = listener.accept().await?;
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
                cmd.execute(&db).await
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
