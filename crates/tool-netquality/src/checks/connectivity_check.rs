use crate::models::{ConnectivityResult, NetQualityConfig};
use crate::runtime_state::LoopState;
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use std::time::Instant;
use tracing::trace;

pub async fn run_connectivity_check(
    config: &NetQualityConfig,
    state: &mut LoopState,
) -> Result<ConnectivityResult> {
    let client = Client::builder()
        .timeout(config.connectivity.timeout)
        .build()
        .context("Failed to build HTTP client")?;

    let start_time = Instant::now();
    let total_urls = config.connectivity.urls.len();
    let mut attempts = 0;
    let mut success = false;
    let mut result = "timeout".to_string();
    let mut selected_url = String::new();

    while attempts < total_urls {
        let index = (state.current_url_index() + attempts) % total_urls;
        let url = &config.connectivity.urls[index];
        trace!("Connectivity check against {}", url);
        let response = client.get(url).send().await;
        match response {
            Ok(resp) => {
                success = true;
                selected_url = url.to_string();
                result = resp.status().as_u16().to_string();
                state.update_next_url_index((index + 1) % total_urls);
                break;
            }
            Err(error) => {
                selected_url = url.to_string();
                if error.is_timeout() {
                    result = "timeout".to_string();
                } else {
                    result = "error".to_string();
                }
                attempts += 1;
            }
        }
    }

    if !success {
        state.update_next_url_index((state.current_url_index() + 1) % total_urls);
    }

    let elapsed_ms = start_time.elapsed().as_millis() as i64;
    Ok(ConnectivityResult {
        timestamp: Utc::now(),
        url: selected_url,
        result,
        elapsed_ms,
        success,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConnectivityConfig, NotificationConfig, SpeedConfig, StorageConfig, ThresholdCategory,
        Thresholds,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    fn build_config(urls: Vec<String>, timeout: Duration) -> NetQualityConfig {
        NetQualityConfig {
            connectivity: ConnectivityConfig {
                delay: Duration::from_secs(1),
                timeout,
                outage_backoff: Duration::from_secs(1),
                outage_backoff_max: Duration::from_secs(5),
                urls,
            },
            speed: SpeedConfig {
                expected_download_mbps: 100.0,
                expected_upload_mbps: None,
                delay: Duration::from_secs(60),
                download_thresholds: Thresholds::default_thresholds(),
                upload_thresholds: Thresholds::default_thresholds(),
                speedtest_cli_path: None,
            },
            notifications: NotificationConfig {
                telegram: None,
                otel_endpoint: None,
                min_download_threshold: ThresholdCategory::Medium,
                min_upload_threshold: ThresholdCategory::Slow,
            },
            storage: StorageConfig {
                db_path: std::env::temp_dir().join("netquality-connectivity-test.db"),
                cleanup_enabled: false,
                cleanup_interval: Duration::from_secs(86_400),
            },
        }
    }

    fn start_http_server(status: u16) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("server addr");
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 {} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    status
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        (format!("http://{}", addr), handle)
    }

    fn unused_port_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind unused port");
        let port = listener.local_addr().expect("port").port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn connectivity_check_success_updates_state() {
        let (server_url, handle) = start_http_server(204);
        let fallback_url = "http://127.0.0.1:1".to_string();
        let config = build_config(
            vec![server_url.clone(), fallback_url],
            Duration::from_secs(1),
        );
        let mut state = LoopState::new(&config);

        let result = run_connectivity_check(&config, &mut state)
            .await
            .expect("connectivity check should succeed");

        let _ = handle.join();
        assert!(result.success);
        assert_eq!(result.url, server_url);
        assert_eq!(result.result, "204");
        assert_eq!(state.current_url_index(), 1);
    }

    #[tokio::test]
    async fn connectivity_check_failure_advances_url_index() {
        let config = build_config(
            vec![unused_port_url(), unused_port_url()],
            Duration::from_millis(150),
        );
        let mut state = LoopState::new(&config);

        let result = run_connectivity_check(&config, &mut state)
            .await
            .expect("connectivity check should complete");

        assert!(!result.success);
        assert_eq!(result.url, config.connectivity.urls[1]);
        assert_eq!(state.current_url_index(), 1);
    }
}
