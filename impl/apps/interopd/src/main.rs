//! `interopd`：Xross Interop 的 headless supervisor / 控制 API 宿主（最小骨架）。
//!
//! T09 只交付受控本地控制端点：Unix domain socket（0600）之上的
//! 长度前缀 JSON-RPC（见 interop-ipc）。无 HTTP 公网 listener；Windows
//! named pipe 载体在后续任务补齐（平台层）。

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)] // 域错误 unboxed（workspace 统一决策）

use interop_ipc::auth::TokenStore;
use interop_ipc::frame::read_frame;
use interop_ipc::server::IpcConnection;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

fn main() {
    let socket = std::env::var("INTEROPD_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut d = std::env::temp_dir();
            d.push(format!("interopd-{}.sock", std::process::id()));
            d
        });
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)
        .unwrap_or_else(|e| panic!("绑定 {socket:?} 失败: {e}"));
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .expect("设置 socket 0600");
    eprintln!("interopd: listening on {}", socket.display());

    // token 注册表：v0.1 骨架从受控文件逐行读 `token subject scope...`；
    // 真实秘密保管走 host secret store（docs/05 §6），本骨架不生成秘密。
    let tokens = load_tokens();

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut conn = IpcConnection::new("interopd-0.1");
        loop {
            match read_frame(&mut stream) {
                Ok(Some(frame)) => match conn.handle_payload(&frame, &tokens) {
                    Ok(resp) => {
                        let mut out = Vec::new();
                        interop_ipc::frame::encode_frame(&resp, &mut out);
                        if use_std_write(&mut stream, &out).is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // 帧级/协议级违规：关闭连接
                },
                Ok(None) => break,       // 对端干净关闭
                Err(_) => break,         // 坏帧：关闭
            }
        }
    }
}

fn load_tokens() -> TokenStore {
    let mut store = TokenStore::new();
    if let Ok(content) = std::fs::read_to_string(
        std::env::var("INTEROPD_TOKENS").unwrap_or_default(),
    ) {
        for line in content.lines() {
            let mut parts = line.split_whitespace();
            if let (Some(token), Some(subject)) = (parts.next(), parts.next()) {
                store.issue_with(token, subject, parts.map(String::from).collect());
            }
        }
    }
    store
}

fn use_std_write(
    stream: &mut std::os::unix::net::UnixStream,
    buf: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;
    stream.write_all(buf)
}
