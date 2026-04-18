#[derive(Debug, Clone)]
pub enum Frame {
    Simple(String),    // +OK\r\n
    Error(String) ,    // -ERR message\r\n
    Integer(i64),      // :10\r\n
    Bulk(String),      // $5\r\nhello\r\n
    Array(Vec<Frame>), // *2\r\n$3\r\nGET\r\n$1\r\na\r\n
    Null,              // $-1\r\n
}

impl Frame {
    // 将 Frame 转为发送给客户端的字节流
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Frame::Simple(s) => format!("+{}\r\n", s).into_bytes(),
            Frame::Error(e) => format!("-{}\r\n", e).into_bytes(),
            Frame::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            Frame::Bulk(s) => format!("${}\r\n{}\r\n", s.len(), s).into_bytes(),
            Frame::Null => "$-1\r\n".to_string().into_bytes(),
            Frame::Array(frames) => {
                let mut out = format!("*{}\r\n", frames.len()).into_bytes();
                for frame in frames {
                    out.extend(frame.to_bytes());
                }
                out
            }
        }
    }
}