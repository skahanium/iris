//! feed 测试专用支撑：本地 HTTP/1.1 测试服务器与宽松网门。
//!
//! 生产路径的 SSRF 校验不允许连接本地服务器，因此测试通过
//! [`TestNetGate`] 注入「允许 127.0.0.1 明文 HTTP、拒绝其他私网」的网门，
//! 使有界获取 / 重定向 / 304 / 超限等行为可以在无外部网络下确定性验证。

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::error::{AppError, AppResult};
use crate::feed::fetch::FeedNetGate;

/// 测试服务器的请求快照（供断言请求头等）。
#[derive(Debug, Clone)]
pub struct TestRequest {
    // 阶段 2 Task 2.4 discovery 测试将消费 method/path；届时移除标注。
    #[allow(dead_code)]
    pub method: String,
    #[allow(dead_code)]
    pub path: String,
    pub headers: HashMap<String, String>,
}

/// 测试服务器响应。
#[derive(Debug, Clone)]
pub struct TestResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl TestResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// 每个请求按顺序消费的响应队列；耗尽后返回 404。
#[derive(Debug)]
pub struct TestServer {
    pub addr: SocketAddr,
    pub requests: Arc<std::sync::Mutex<Vec<TestRequest>>>,
    responses: Arc<std::sync::Mutex<std::collections::VecDeque<TestResponse>>>,
    /// 可选：对每个请求延迟响应（毫秒），用于超时测试。
    #[allow(dead_code)]
    delay_millis: u64,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl TestServer {
    pub async fn start() -> Self {
        Self::start_with_delay(0).await
    }

    pub async fn start_with_delay(delay_millis: u64) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let requests: Arc<std::sync::Mutex<Vec<TestRequest>>> = Default::default();
        let responses: Arc<std::sync::Mutex<std::collections::VecDeque<TestResponse>>> =
            Default::default();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let requests_clone = requests.clone();
        let responses_clone = responses.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { continue };
                        let requests = requests_clone.clone();
                        let responses = responses_clone.clone();
                        let delay = delay_millis;
                        tokio::spawn(async move {
                            let mut buf = Vec::new();
                            let mut tmp = [0u8; 4096];
                            loop {
                                match socket.read(&mut tmp).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        buf.extend_from_slice(&tmp[..n]);
                                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                            let head = String::from_utf8_lossy(&buf);
                            let mut lines = head.split("\r\n");
                            let request_line = lines.next().unwrap_or("").to_string();
                            let mut headers = HashMap::new();
                            for line in lines {
                                if let Some((key, value)) = line.split_once(':') {
                                    headers.insert(
                                        key.trim().to_ascii_lowercase(),
                                        value.trim().to_string(),
                                    );
                                }
                            }
                            let mut parts = request_line.split_whitespace();
                            let method = parts.next().unwrap_or("GET").to_string();
                            let path = parts.next().unwrap_or("/").to_string();
                            if let Ok(mut guard) = requests.lock() {
                                guard.push(TestRequest {
                                    method,
                                    path,
                                    headers,
                                });
                            }

                            if delay > 0 {
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                            }
                            let response = responses
                                .lock()
                                .map(|mut queue| queue.pop_front())
                                .ok()
                                .flatten()
                                .unwrap_or_else(|| TestResponse::new(404, "not found"));

                            let reason = match response.status {
                                200 => "OK",
                                301 => "Moved Permanently",
                                302 => "Found",
                                304 => "Not Modified",
                                404 => "Not Found",
                                500 => "Internal Server Error",
                                _ => "Status",
                            };
                            let mut out = format!(
                                "HTTP/1.1 {} {}\r\nConnection: close\r\n",
                                response.status, reason
                            );
                            let declares_length = response
                                .headers
                                .iter()
                                .any(|(key, _)| key.eq_ignore_ascii_case("content-length"));
                            for (key, value) in &response.headers {
                                out.push_str(&format!("{key}: {value}\r\n"));
                            }
                            if declares_length {
                                // 测试显式声明 CL 时按声明发送。
                                out.push_str("\r\n");
                            } else {
                                // 未声明 Content-Length 时按 close 定界，
                                // 供流式超限（流中截断）测试。
                                out.push_str("\r\n");
                            }
                            let _ = socket.write_all(out.as_bytes()).await;
                            let _ = socket.write_all(&response.body).await;
                            let _ = socket.flush().await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            requests,
            responses,
            delay_millis,
            shutdown: Some(shutdown_tx),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub fn queue(&self, response: TestResponse) {
        self.responses
            .lock()
            .expect("queue test response")
            .push_back(response);
    }

    pub fn requests_snapshot(&self) -> Vec<TestRequest> {
        self.requests
            .lock()
            .expect("snapshot test requests")
            .clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// 测试网门：允许 `http://127.0.0.1`（含任意端口）与任意 https 域名，
/// 拒绝其他私网地址；`build_client` 构造不带代理、可配置超时的明文客户端。
pub struct TestNetGate {
    pub timeout: Duration,
}

impl Default for TestNetGate {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
        }
    }
}

impl TestNetGate {
    fn validate_host(host: &str) -> AppResult<()> {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host == "127.0.0.1" {
            return Ok(());
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            let blocked = match ip {
                IpAddr::V4(v4) => {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                }
                IpAddr::V6(v6) => {
                    v6.is_loopback() || v6.is_unspecified() || v6.is_unicast_link_local()
                }
            };
            if blocked {
                return Err(AppError::msg("test gate: private address rejected"));
            }
        }
        Ok(())
    }
}

impl FeedNetGate for TestNetGate {
    fn validate_url(&self, url: &str) -> AppResult<()> {
        let parsed = reqwest::Url::parse(url).map_err(|_| AppError::msg("test gate: bad url"))?;
        if !parsed.username().is_empty() {
            return Err(AppError::msg("test gate: userinfo rejected"));
        }
        let scheme_ok = matches!(parsed.scheme(), "http" | "https");
        if !scheme_ok {
            return Err(AppError::msg("test gate: scheme rejected"));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| AppError::msg("test gate: no host"))?;
        Self::validate_host(host)
    }

    async fn resolve_public_addrs(&self, host: &str) -> AppResult<Vec<IpAddr>> {
        if host == "127.0.0.1" {
            return Ok(vec!["127.0.0.1".parse().expect("loopback")]);
        }
        // 测试环境不解析真实 DNS；非 loopback 域名直接放行一个公网占位地址，
        // 由 build_client 的 pinning 承担连接目标。
        let fallback: IpAddr = if host.contains(':') {
            "2606:2800:220:1:248:1893:25c8:1946".parse().expect("v6")
        } else {
            "93.184.216.34".parse().expect("v4")
        };
        Ok(vec![fallback])
    }

    fn build_client(&self, host: &str, port: u16, addrs: &[IpAddr]) -> AppResult<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .https_only(false)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.timeout);
        for addr in addrs {
            builder = builder.resolve(host, SocketAddr::new(*addr, port));
        }
        builder
            .build()
            .map_err(|e| AppError::msg(format!("test gate client build failed: {e}")))
    }
}
