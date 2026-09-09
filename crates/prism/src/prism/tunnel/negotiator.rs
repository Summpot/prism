//! Automatic protocol negotiation and fallback engine for Prism tunnel mode.
//!
//! Features:
//! 1. Accepts hostnames without protocol prefix or port (e.g. "tunnel.example.com").
//! 2. Automatically queries DNS SVCB/HTTPS records (RFC 9460) via DoH.
//! 3. Orders candidates by priority (WebTransport > QUIC > WebSocket > TCP).
//! 4. Sequentially dials candidate protocols with timeout and automatic fallback.

use std::{sync::Arc, time::Duration};
use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::prism::tunnel::transport::{
    TransportDialOptions, TransportSession, parse_transport, transport_by_name,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEndpoint {
    pub priority: u16,
    pub target_host: String,
    pub port: u16,
    pub protocol: String,
    pub alpn: String,
}

/// Default list of public DoH providers queried concurrently for SVCB/HTTPS discovery.
///
/// Includes diverse anycast IP endpoints (to bypass DNS bootstrapping) and global/regional providers.
pub const DEFAULT_DOH_PROVIDERS: &[&str] = &[
    "https://1.1.1.1/dns-query",
    "https://cloudflare-dns.com/dns-query",
    "https://8.8.8.8/resolve",
    "https://dns.google/resolve",
    "https://223.5.5.5/resolve",
    "https://dns.alidns.com/resolve",
    "https://dns.adguard-dns.com/resolve",
    "https://doh.360.cn/resolve",
];

#[derive(Debug, Deserialize)]
struct DohResponse {
    #[allow(dead_code)]
    #[serde(rename = "Status", alias = "status")]
    status: Option<i32>,
    #[serde(rename = "Answer", alias = "answer", default)]
    answer: Vec<DohAnswer>,
}

#[derive(Debug, Deserialize)]
struct DohAnswer {
    #[serde(rename = "type", alias = "Type")]
    record_type: Option<u16>,
    #[serde(rename = "data", alias = "Data")]
    data: String,
}

/// Parses an RFC 9460 SVCB / HTTPS presentation format record data string.
///
/// Example: `1 . alpn=h3 port=7000` or `1 target.domain alpn="h3,http/1.1" port=8443`
pub fn parse_svcb_record(query_domain: &str, data: &str) -> Vec<CandidateEndpoint> {
    let parts: Vec<&str> = data.split_whitespace().collect();
    if parts.len() < 2 {
        return Vec::new();
    }

    let priority: u16 = match parts[0].parse() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    if priority == 0 {
        // AliasMode: parts[1] is target alias.
        return Vec::new();
    }

    let target = parts[1];
    let host = if target == "." || target.is_empty() {
        query_domain.trim().trim_end_matches('.').to_string()
    } else {
        target.trim().trim_end_matches('.').to_string()
    };

    let mut port: u16 = 443;
    let mut alpns: Vec<String> = Vec::new();

    // Reconstruct remainder to handle quoted values with spaces or commas
    let params_str = parts[2..].join(" ");
    let mut in_quotes = false;
    let mut current = String::new();
    let mut param_tokens = Vec::new();

    for ch in params_str.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == ' ' && !in_quotes {
            if !current.is_empty() {
                param_tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        param_tokens.push(current);
    }

    for token in param_tokens {
        let (k, v) = match token.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim().trim_matches('"')),
            None => (token.trim(), ""),
        };

        match k {
            "port" => {
                if let Ok(p) = v.parse::<u16>() {
                    port = p;
                }
            }
            "alpn" => {
                for item in v.split(',') {
                    let clean = item.trim();
                    if !clean.is_empty() {
                        alpns.push(clean.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if alpns.is_empty() {
        alpns.push("h3".to_string());
    }

    let mut candidates = Vec::new();
    for alpn in alpns {
        let protocol = map_alpn_to_protocol(&alpn);
        candidates.push(CandidateEndpoint {
            priority,
            target_host: host.clone(),
            port,
            protocol,
            alpn,
        });
    }

    candidates
}

/// Maps an ALPN string to a Prism transport name.
pub fn map_alpn_to_protocol(alpn: &str) -> String {
    let lower = alpn.to_ascii_lowercase();
    if lower == "h3" || lower.starts_with("h3-") || lower == "webtransport" {
        "webtransport".to_string()
    } else if lower.contains("quic") || lower == "prism-tunnel" {
        "quic".to_string()
    } else if lower.contains("ws") || lower == "http/1.1" || lower == "h2" {
        "websocket".to_string()
    } else if lower.contains("tcp") {
        "tcp".to_string()
    } else {
        "webtransport".to_string()
    }
}

/// Resolves candidate connection endpoints for the given `server_addr`.
pub async fn resolve_candidates(
    server_addr: &str,
    configured_transport: Option<&str>,
    custom_doh: Option<&[&str]>,
) -> Result<Vec<CandidateEndpoint>> {
    let raw = server_addr.trim();

    // 1. Check for protocol scheme prefix (e.g. "wt://", "quic://", "ws://", "wss://", "tcp://")
    let (explicit_proto, without_scheme) = if let Some((scheme, rest)) = raw.split_once("://") {
        let proto = match scheme.to_ascii_lowercase().as_str() {
            "wt" | "webtransport" => Some("webtransport"),
            "quic" | "prism" => Some("quic"),
            "ws" | "wss" | "websocket" => Some("websocket"),
            "tcp" => Some("tcp"),
            "kcp" | "udp" => Some("udp"),
            _ => None,
        };
        (proto, rest)
    } else {
        (None, raw)
    };

    let without_path = without_scheme.split('/').next().unwrap_or(without_scheme).trim();

    // 2. Check if host has an explicit port (e.g. "example.com:7000" or "127.0.0.1:25565")
    let (host, explicit_port) = match without_path.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
            if let Ok(port_num) = p.parse::<u16>() {
                let clean_host = h.trim_start_matches('[').trim_end_matches(']');
                (clean_host.to_string(), Some(port_num))
            } else {
                (without_path.to_string(), None)
            }
        }
        _ => (without_path.to_string(), None),
    };

    let forced_transport = explicit_proto
        .or_else(|| configured_transport.filter(|t| !t.trim().is_empty() && *t != "auto"));

    // 3. If explicit port or explicit transport is provided:
    if let Some(port) = explicit_port {
        if let Some(proto) = forced_transport {
            let p = parse_transport(proto)?;
            return Ok(vec![CandidateEndpoint {
                priority: 1,
                target_host: host,
                port,
                protocol: p,
                alpn: "".to_string(),
            }]);
        }

        // Auto mode with explicit port: probe in priority order
        return Ok(vec![
            CandidateEndpoint {
                priority: 1,
                target_host: host.clone(),
                port,
                protocol: "webtransport".to_string(),
                alpn: "h3".to_string(),
            },
            CandidateEndpoint {
                priority: 2,
                target_host: host.clone(),
                port,
                protocol: "quic".to_string(),
                alpn: "prism-tunnel".to_string(),
            },
            CandidateEndpoint {
                priority: 3,
                target_host: host.clone(),
                port,
                protocol: "websocket".to_string(),
                alpn: "http/1.1".to_string(),
            },
            CandidateEndpoint {
                priority: 4,
                target_host: host,
                port,
                protocol: "tcp".to_string(),
                alpn: "".to_string(),
            },
        ]);
    }

    // 4. No port specified: attempt SVCB / HTTPS discovery via concurrent Multi-DoH first
    let doh_endpoints: Vec<&str> = if let Some(custom) = custom_doh {
        if custom.is_empty() {
            DEFAULT_DOH_PROVIDERS.to_vec()
        } else {
            custom.to_vec()
        }
    } else {
        DEFAULT_DOH_PROVIDERS.to_vec()
    };

    tracing::debug!(
        domain = %host,
        providers = doh_endpoints.len(),
        "Querying multiple DoH providers concurrently for SVCB/HTTPS records"
    );

    let mut candidates = query_concurrent_doh_candidates(&host, &doh_endpoints).await;

    // 5. If DoH resolution failed or returned empty: Fall back to Local DNS (System Resolver)
    if candidates.is_empty() {
        tracing::info!(
            domain = %host,
            "DoH query failed or returned no records; falling back to Local DNS query (system resolver)"
        );
        match query_local_dns_candidates(&host).await {
            Ok(c) if !c.is_empty() => {
                tracing::info!(
                    domain = %host,
                    count = c.len(),
                    "Successfully resolved SVCB candidate(s) via Local DNS"
                );
                candidates = c;
            }
            Ok(_) => {
                tracing::debug!(domain = %host, "Local DNS returned no HTTPS/SVCB records");
            }
            Err(err) => {
                tracing::warn!(
                    domain = %host,
                    err = %err,
                    "Local DNS query for HTTPS/SVCB failed"
                );
            }
        }
    }

    if let Some(proto) = forced_transport {
        let p = parse_transport(proto)?;
        candidates.retain(|c| c.protocol == p);
        if candidates.is_empty() {
            candidates.push(CandidateEndpoint {
                priority: 1,
                target_host: host.clone(),
                port: 443,
                protocol: p,
                alpn: "".to_string(),
            });
        }
    }

    if candidates.is_empty() {
        tracing::debug!(
            domain = %host,
            "No SVCB/HTTPS records found via DoH or Local DNS; using default protocol priority on port 443"
        );
        // Default fallback list on port 443 (standard HTTPS/QUIC port)
        candidates = vec![
            CandidateEndpoint {
                priority: 1,
                target_host: host.clone(),
                port: 443,
                protocol: "webtransport".to_string(),
                alpn: "h3".to_string(),
            },
            CandidateEndpoint {
                priority: 2,
                target_host: host.clone(),
                port: 443,
                protocol: "quic".to_string(),
                alpn: "prism-tunnel".to_string(),
            },
            CandidateEndpoint {
                priority: 3,
                target_host: host.clone(),
                port: 443,
                protocol: "websocket".to_string(),
                alpn: "http/1.1".to_string(),
            },
            CandidateEndpoint {
                priority: 4,
                target_host: host,
                port: 443,
                protocol: "tcp".to_string(),
                alpn: "".to_string(),
            },
        ];
    } else {
        candidates.sort_by_key(|c| c.priority);
    }

    Ok(candidates)
}

/// Queries Local DNS (System DNS / Local Resolver via UDP/TCP) for HTTPS (type 65) or SVCB (type 64) records.
pub async fn query_local_dns_candidates(domain: &str) -> Result<Vec<CandidateEndpoint>> {
    use hickory_resolver::Resolver;
    use hickory_resolver::proto::rr::{RecordType, RData};

    let resolver = Resolver::builder_tokio()
        .context("failed to initialize local DNS resolver from system configuration")?
        .build()?;

    let clean_domain = domain.trim().trim_end_matches('.');
    let mut candidates = Vec::new();

    // 1. Try HTTPS (type 65) via Local DNS
    if let Ok(lookup) = resolver.lookup(clean_domain, RecordType::HTTPS).await {
        for record in lookup.answers() {
            if let RData::HTTPS(svcb) = &record.data {
                let svcb_str = svcb.to_string();
                candidates.extend(parse_svcb_record(clean_domain, &svcb_str));
            } else {
                let rec_str = record.to_string();
                if let Some((_, rdata)) = rec_str.split_once("HTTPS ") {
                    candidates.extend(parse_svcb_record(clean_domain, rdata.trim()));
                }
            }
        }
    }

    // 2. If no HTTPS records found, try SVCB (type 64) via Local DNS
    if candidates.is_empty() {
        if let Ok(lookup) = resolver.lookup(clean_domain, RecordType::SVCB).await {
            for record in lookup.answers() {
                if let RData::SVCB(svcb) = &record.data {
                    let svcb_str = svcb.to_string();
                    candidates.extend(parse_svcb_record(clean_domain, &svcb_str));
                } else {
                    let rec_str = record.to_string();
                    if let Some((_, rdata)) = rec_str.split_once("SVCB ") {
                        candidates.extend(parse_svcb_record(clean_domain, rdata.trim()));
                    }
                }
            }
        }
    }

    Ok(candidates)
}

/// Queries multiple DoH endpoints concurrently in parallel.
///
/// Returns candidates from the fastest DoH provider that yields a non-empty result.
async fn query_concurrent_doh_candidates(
    domain: &str,
    doh_urls: &[&str],
) -> Vec<CandidateEndpoint> {
    use futures_util::stream::FuturesUnordered;
    use futures_util::StreamExt;

    if doh_urls.is_empty() {
        return Vec::new();
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(3500))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(err = %e, "Failed to build HTTP client for DoH queries");
            return Vec::new();
        }
    };

    let mut tasks = FuturesUnordered::new();
    for &url in doh_urls {
        let client_clone = client.clone();
        let domain_str = domain.to_string();
        tasks.push(async move {
            let res = query_doh_candidates(&client_clone, &domain_str, url).await;
            (url, res)
        });
    }

    let mut found = Vec::new();

    while let Some((url, res)) = tasks.next().await {
        match res {
            Ok(c) if !c.is_empty() => {
                tracing::info!(
                    domain = %domain,
                    doh = %url,
                    count = c.len(),
                    "Resolved SVCB candidates via concurrent DoH"
                );
                found = c;
                break;
            }
            Ok(_) => {
                tracing::debug!(domain = %domain, doh = %url, "DoH returned 0 HTTPS/SVCB records");
            }
            Err(err) => {
                tracing::debug!(domain = %domain, doh = %url, err = %err, "DoH query failed");
            }
        }
    }

    found
}

/// Queries a single DoH provider for HTTPS (type 65) or SVCB (type 64) records.
async fn query_doh_candidates(
    client: &reqwest::Client,
    domain: &str,
    doh_url: &str,
) -> Result<Vec<CandidateEndpoint>> {
    let clean_domain = domain.trim().trim_end_matches('.');
    let url = format!("{doh_url}?name={clean_domain}&type=HTTPS");

    let resp = client
        .get(&url)
        .header("Accept", "application/dns-json, application/json")
        .header("User-Agent", "prism/0.1.0")
        .send()
        .await
        .context("query DoH HTTPS record")?;

    if !resp.status().is_success() {
        bail!("DoH returned status {}", resp.status());
    }

    let doh: DohResponse = resp.json().await.context("decode DoH JSON")?;
    let mut candidates = Vec::new();

    for ans in doh.answer {
        if ans.record_type == Some(65) || ans.record_type.is_none() {
            let parsed = parse_svcb_record(clean_domain, &ans.data);
            candidates.extend(parsed);
        }
    }

    // Fallback: try type 64 (SVCB) if HTTPS was empty
    if candidates.is_empty() {
        let svcb_url = format!("{doh_url}?name={clean_domain}&type=SVCB");
        if let Ok(resp) = client
            .get(&svcb_url)
            .header("Accept", "application/dns-json, application/json")
            .header("User-Agent", "prism/0.1.0")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(doh) = resp.json::<DohResponse>().await {
                    for ans in doh.answer {
                        if ans.record_type == Some(64) || ans.record_type.is_none() {
                            let parsed = parse_svcb_record(clean_domain, &ans.data);
                            candidates.extend(parsed);
                        }
                    }
                }
            }
        }
    }

    Ok(candidates)
}

/// Dials a list of candidates in priority order with automatic fallback.
pub async fn dial_with_fallback(
    candidates: &[CandidateEndpoint],
    dial_timeout: Duration,
    opts: &TransportDialOptions,
) -> Result<(Arc<dyn TransportSession>, CandidateEndpoint)> {
    if candidates.is_empty() {
        bail!("no connection candidates available");
    }

    let per_attempt_timeout = (dial_timeout / (candidates.len().max(1) as u32))
        .max(Duration::from_secs(3))
        .min(dial_timeout);

    let mut last_err = anyhow::anyhow!("all candidates failed");

    for candidate in candidates {
        tracing::info!(
            protocol = %candidate.protocol,
            host = %candidate.target_host,
            port = candidate.port,
            priority = candidate.priority,
            "Tunnel dial: trying candidate protocol..."
        );

        let tr = match transport_by_name(&candidate.protocol) {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!(err = %e, "Skipping unsupported transport");
                continue;
            }
        };

        let target_addr = format!("{}:{}", candidate.target_host, candidate.port);
        let mut attempt_opts = opts.clone();

        // Populate server_name for SNI / WebTransport verification if empty
        if attempt_opts.quic.server_name.is_empty() {
            attempt_opts.quic.server_name = candidate.target_host.clone();
        }
        if attempt_opts.websocket.server_name.is_empty() {
            attempt_opts.websocket.server_name = candidate.target_host.clone();
        }
        if attempt_opts.webtransport.server_name.is_empty() {
            attempt_opts.webtransport.server_name = candidate.target_host.clone();
        }

        let dial_future = tr.dial(&target_addr, attempt_opts);
        match tokio::time::timeout(per_attempt_timeout, dial_future).await {
            Ok(Ok(sess)) => {
                tracing::info!(
                    protocol = %candidate.protocol,
                    host = %candidate.target_host,
                    port = candidate.port,
                    "Tunnel dial: successfully established connection"
                );
                return Ok((sess, candidate.clone()));
            }
            Ok(Err(err)) => {
                tracing::warn!(
                    protocol = %candidate.protocol,
                    target = %target_addr,
                    err = %err,
                    "Tunnel dial: candidate failed; falling back to next protocol"
                );
                last_err = err;
            }
            Err(_) => {
                tracing::warn!(
                    protocol = %candidate.protocol,
                    target = %target_addr,
                    timeout_secs = per_attempt_timeout.as_secs(),
                    "Tunnel dial: candidate timed out; falling back to next protocol"
                );
                last_err = anyhow::anyhow!("connection to {} timed out", target_addr);
            }
        }
    }

    Err(last_err.context("all connection candidates failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_svcb_record_single_param() {
        let data = "1 . alpn=h3 port=7000";
        let candidates = parse_svcb_record("example.com", data);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].priority, 1);
        assert_eq!(candidates[0].target_host, "example.com");
        assert_eq!(candidates[0].port, 7000);
        assert_eq!(candidates[0].protocol, "webtransport");
    }

    #[test]
    fn test_parse_svcb_record_multi_alpn_and_target() {
        let data = "2 target.prism.com alpn=\"h3,prism-quic,http/1.1\" port=8443";
        let candidates = parse_svcb_record("example.com", data);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].priority, 2);
        assert_eq!(candidates[0].target_host, "target.prism.com");
        assert_eq!(candidates[0].port, 8443);
        assert_eq!(candidates[0].protocol, "webtransport");

        assert_eq!(candidates[1].protocol, "quic");
        assert_eq!(candidates[2].protocol, "websocket");
    }

    #[test]
    fn test_parse_svcb_record_default_port() {
        let data = "1 . alpn=h3";
        let candidates = parse_svcb_record("example.com", data);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].port, 443);
        assert_eq!(candidates[0].protocol, "webtransport");
    }

    #[test]
    fn test_map_alpn_to_protocol() {
        assert_eq!(map_alpn_to_protocol("h3"), "webtransport");
        assert_eq!(map_alpn_to_protocol("webtransport"), "webtransport");
        assert_eq!(map_alpn_to_protocol("prism-quic"), "quic");
        assert_eq!(map_alpn_to_protocol("prism-tunnel"), "quic");
        assert_eq!(map_alpn_to_protocol("http/1.1"), "websocket");
        assert_eq!(map_alpn_to_protocol("prism-tcp"), "tcp");
    }

    #[tokio::test]
    async fn test_resolve_candidates_fallback_on_invalid_doh() {
        // An invalid DoH URL will fail, triggering fallback to Local DNS and default port 443 candidates
        let candidates = resolve_candidates("test-fallback.invalid", None, Some(&["http://127.0.0.1:9"]))
            .await
            .unwrap();
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].port, 443);
        assert_eq!(candidates[0].protocol, "webtransport");
    }

    #[tokio::test]
    async fn test_resolve_candidates_concurrent_doh_picks_fastest_responder() {
        use axum::{routing::get, Json, Router};
        use serde_json::json;
        use tokio::net::TcpListener;

        // Slow mock DoH server (sleeps 3s)
        let slow_app = Router::new().route(
            "/dns-query",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                Json(json!({ "Status": 0, "Answer": [] }))
            }),
        );
        let slow_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let slow_addr = slow_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(slow_listener, slow_app).await.unwrap();
        });

        // Fast mock DoH server (responds immediately with SVCB candidate)
        let fast_app = Router::new().route(
            "/dns-query",
            get(|| async {
                Json(json!({
                    "Status": 0,
                    "Answer": [
                        {
                            "name": "example.com.",
                            "type": 65,
                            "data": "1 . alpn=h3 port=7000"
                        }
                    ]
                }))
            }),
        );
        let fast_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fast_addr = fast_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(fast_listener, fast_app).await.unwrap();
        });

        let slow_url = format!("http://{slow_addr}/dns-query");
        let fast_url = format!("http://{fast_addr}/dns-query");

        let start = std::time::Instant::now();
        let candidates = resolve_candidates(
            "example.com",
            None,
            Some(&[&slow_url, &fast_url]),
        )
        .await
        .unwrap();

        // Must complete swiftly without being blocked by the 3s slow server
        assert!(start.elapsed() < Duration::from_millis(1500));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].port, 7000);
        assert_eq!(candidates[0].protocol, "webtransport");
    }
}
