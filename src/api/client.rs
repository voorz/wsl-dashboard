// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use ureq::{Agent, tls::{TlsConfig, TlsProvider}};
use std::time::Duration;
use std::thread;
use tracing::{trace, debug, error};


#[derive(Debug, Deserialize, Serialize)]
pub struct ApiResponse<T> {
    pub err: i32,
    pub msg: String,
    pub data: T,
}

pub struct WslUiClient {
    api1_url: String,
    api2_url: String,
    agent: Agent,
}

impl WslUiClient {
    pub fn new() -> Self {
        let mut config = Agent::config_builder();
        if cfg!(debug_assertions) {
            if let Ok(_tls_connector) = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
            {
                config = config.tls_config(
                    TlsConfig::builder()
                        .provider(TlsProvider::NativeTls)
                        .disable_verification(true)
                        .build()
                );
                trace!("Developer mode: HTTPS certificate verification is disabled (-k)");
            }
        }
        let agent = Agent::new_with_config(config.build());

        Self {
            api1_url: crate::app::API1_URL.to_string(),
            api2_url: crate::app::API2_URL.to_string(),
            agent,
        }
    }

    #[allow(dead_code)]
    pub fn request_api1<T>(&self, method: &str, uri: &str, body: Option<serde_json::Value>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        self.request_api1_with_timeout(method, uri, body, None)
    }

    pub fn request_api1_with_timeout<T>(&self, method: &str, uri: &str, body: Option<serde_json::Value>, timeout_ms: Option<u64>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        let url = format!("{}{}", self.api1_url, uri);
        self.request_url(method, &url, body, timeout_ms)
    }

    #[allow(dead_code)]
    pub fn request_api2<T>(&self, method: &str, uri: &str, body: Option<serde_json::Value>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        self.request_api2_with_timeout(method, uri, body, None)
    }

    pub fn request_api2_with_timeout<T>(&self, method: &str, uri: &str, body: Option<serde_json::Value>, timeout_ms: Option<u64>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        let url = format!("{}{}", self.api2_url, uri);
        self.request_url(method, &url, body, timeout_ms)
    }

    pub fn request_full_url<T>(&self, method: &str, url: &str, body: Option<serde_json::Value>, timeout_ms: Option<u64>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        self.request_url(method, url, body, timeout_ms)
    }

    fn request_url<T>(&self, method: &str, url: &str, body: Option<serde_json::Value>, timeout_ms: Option<u64>) -> Result<(ApiResponse<T>, Option<String>), String>
    where
        T: for<'de> Deserialize<'de> + std::fmt::Debug,
    {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(5000));
        let do_request = || -> Result<(ApiResponse<T>, Option<String>), String> {
            trace!("WSLUI API Request: method={}, url={}, body={:?}", method, url, body);

            let is_post = method.to_uppercase().as_str() == "POST";

            let req = if is_post {
                self.agent.post(url)
                    .config()
                    .timeout_global(Some(timeout))
                    .build()
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            } else {
                self.agent.get(url)
                    .config()
                    .timeout_global(Some(timeout))
                    .build()
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                    .force_send_body()
            };

            let resp = if let Some(ref b) = body {
                req.send_json(b)
            } else {
                req.send_empty()
            };

            let mut resp = resp.map_err(|e: ureq::Error| {
                let error_msg = e.to_string();
                if Self::is_timeout_error(&error_msg) {
                    error!("WSLUI API Timeout Error: {}", error_msg);
                    "RequestTimeOut".to_string()
                } else {
                    error!("WSLUI API Network Error: {}", error_msg);
                    error_msg
                }
            })?;

            let date_header = resp.headers().get("Date")
                .and_then(|v: &ureq::http::HeaderValue| v.to_str().ok())
                .map(|s: &str| s.to_string());
            let status = resp.status().as_u16();

            let resp_text = resp.body_mut().read_to_string().map_err(|e: ureq::Error| {
                error!("WSLUI API Read Body Error: {}", e);
                e.to_string()
            })?;

            let compact_body = serde_json::from_str::<serde_json::Value>(&resp_text)
                .and_then(|v| serde_json::to_string(&v))
                .unwrap_or(resp_text.clone());
            debug!("WSLUI API Response: status={}, body={}", status, compact_body);

            let clean_text = resp_text.trim_start_matches('\u{FEFF}').trim();

            let api_resp: ApiResponse<T> = serde_json::from_str(clean_text).map_err(|e| {
                error!("WSLUI API JSON Parse Error: {}", e);
                e.to_string()
            })?;

            if api_resp.err != 0 {
                return Err(format!("API business error: err={}, msg={}", api_resp.err, api_resp.msg));
            }

            Ok((api_resp, date_header))
        };

        match do_request() {
            Ok(res) => Ok(res),
            Err(e) => {
                if e == "RequestTimeOut" {
                    debug!("WSLUI API Request timed out. Retrying after 200ms...");
                } else {
                    debug!("WSLUI API Request failed: {}. Retrying after 200ms...", e);
                }
                thread::sleep(Duration::from_millis(200));
                do_request().map_err(|e2| {
                    if e2 == "RequestTimeOut" {
                        error!("WSLUI API Retry timed out");
                    } else {
                        error!("WSLUI API Retry failed: {}", e2);
                    }
                    e2
                })
            }
        }
    }

    fn is_timeout_error(error_msg: &str) -> bool {
        let lower_msg = error_msg.to_lowercase();
        lower_msg.contains("timed out") 
            || lower_msg.contains("timeout")
            || lower_msg.contains("connection timed out")
            || lower_msg.contains("request timed out")
    }
}