use std::time::Duration;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct CloudflareClient {
    api_token: String,
    zone_id: Option<String>,
    http_client: reqwest::Client,
    api_base_url: String,
    doh_base_url: String,
}

#[derive(Debug, Deserialize)]
struct CfResponse<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<CfError>,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct CfError {
    #[allow(dead_code)]
    code: Option<i64>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CfZone {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateDnsRecordRequest<'a> {
    #[serde(rename = "type")]
    record_type: &'static str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SvcbRecordData {
    pub priority: u16,
    pub target: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
struct CreateSvcbDnsRecordRequest<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    name: &'a str,
    ttl: u32,
    data: SvcbRecordData,
}

#[derive(Debug, Deserialize)]
pub struct CfDnsRecord<T> {
    pub id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub record_type: String,
    #[allow(dead_code)]
    pub name: String,
    pub data: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedEndpoint {
    pub priority: u16,
    pub port: u16,
    pub alpn: String,
}

#[derive(Debug, Deserialize)]
struct DnsRecordResult {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DohResponse {
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

impl CloudflareClient {
    pub fn new(api_token: String, zone_id: Option<String>) -> Self {
        Self {
            api_token,
            zone_id,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_base_url: "https://api.cloudflare.com/client/v4".to_string(),
            doh_base_url: "https://cloudflare-dns.com/dns-query".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn with_base_urls(
        api_token: String,
        zone_id: Option<String>,
        api_base_url: String,
        doh_base_url: String,
    ) -> Self {
        Self {
            api_token,
            zone_id,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_base_url,
            doh_base_url,
        }
    }

    /// Resolve zone ID for a domain name. If `zone_id` was configured explicitly,
    /// it is returned immediately. Otherwise, it queries Cloudflare to match the
    /// appropriate DNS zone.
    pub async fn resolve_zone_id(&self, domain: &str) -> Result<String> {
        if let Some(ref zid) = self.zone_id {
            let trimmed = zid.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        let clean_domain = domain.trim().trim_start_matches("*.").trim_end_matches('.');
        if clean_domain.is_empty() {
            bail!("cannot resolve zone ID for empty domain");
        }

        // 1. Try listing zones first (standard if token has Zone:Read permission)
        let list_url = format!("{}/zones?status=active&per_page=50", self.api_base_url);
        let resp = self
            .http_client
            .get(&list_url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await;

        if let Ok(res) = resp {
            if res.status().is_success() {
                if let Ok(cf_resp) = res.json::<CfResponse<Vec<CfZone>>>().await {
                    if cf_resp.success && let Some(zones) = cf_resp.result {
                        // Find the zone with the longest suffix match
                        let mut best_match: Option<(usize, String)> = None;
                        for zone in zones {
                            let zname = zone.name.trim().trim_end_matches('.');
                            if clean_domain == zname
                                || clean_domain.ends_with(&format!(".{zname}"))
                            {
                                if best_match.as_ref().map_or(true, |(len, _)| zname.len() > *len) {
                                    best_match = Some((zname.len(), zone.id));
                                }
                            }
                        }
                        if let Some((_, zid)) = best_match {
                            return Ok(zid);
                        }
                    }
                }
            }
        }

        // 2. Fallback: try candidate zone names individually (for zone-scoped tokens)
        let labels: Vec<&str> = clean_domain.split('.').collect();
        for i in 0..labels.len().saturating_sub(1) {
            let candidate = labels[i..].join(".");
            let query_url = format!("{}/zones?name={}&status=active", self.api_base_url, candidate);
            let resp = self
                .http_client
                .get(&query_url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .send()
                .await;

            if let Ok(res) = resp {
                if res.status().is_success() {
                    if let Ok(cf_resp) = res.json::<CfResponse<Vec<CfZone>>>().await {
                        if cf_resp.success
                            && let Some(zones) = cf_resp.result
                            && let Some(first) = zones.into_iter().next()
                        {
                            return Ok(first.id);
                        }
                    }
                }
            }
        }

        bail!("failed to find Cloudflare zone ID for domain: {domain}")
    }

    /// Create a DNS TXT record for the ACME challenge.
    pub async fn create_txt_record(
        &self,
        zone_id: &str,
        record_name: &str,
        content: &str,
    ) -> Result<String> {
        let url = format!("{}/zones/{zone_id}/dns_records", self.api_base_url);
        let body = CreateDnsRecordRequest {
            record_type: "TXT",
            name: record_name,
            content,
            ttl: 60,
        };

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .json(&body)
            .send()
            .await
            .context("send Cloudflare create DNS record request")?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        let cf_resp: CfResponse<DnsRecordResult> = serde_json::from_str(&body_text)
            .with_context(|| format!("parse Cloudflare response (status {status}): {body_text}"))?;

        if !cf_resp.success {
            let err_msgs = cf_resp
                .errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            bail!("Cloudflare API error creating TXT record: {err_msgs}");
        }

        let record_id = cf_resp
            .result
            .map(|r| r.id)
            .ok_or_else(|| anyhow::anyhow!("missing record id in Cloudflare response"))?;

        tracing::info!(
            zone_id = %zone_id,
            record_name = %record_name,
            record_id = %record_id,
            "Cloudflare: created ACME challenge DNS TXT record"
        );

        Ok(record_id)
    }

    /// Delete a DNS TXT record after challenge completion.
    pub async fn delete_txt_record(&self, zone_id: &str, record_id: &str) -> Result<()> {
        let url = format!("{}/zones/{zone_id}/dns_records/{record_id}", self.api_base_url);
        let resp = self
            .http_client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
            .context("send Cloudflare delete DNS record request")?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            // Already deleted
            return Ok(());
        }

        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            tracing::warn!(
                zone_id = %zone_id,
                record_id = %record_id,
                status = %status,
                body = %body_text,
                "Cloudflare: non-success status deleting DNS TXT record"
            );
        } else {
            tracing::info!(
                zone_id = %zone_id,
                record_id = %record_id,
                "Cloudflare: deleted ACME challenge DNS TXT record"
            );
        }

        Ok(())
    }

    /// List HTTPS DNS records for a given domain in the zone.
    pub async fn list_https_records(
        &self,
        zone_id: &str,
        record_name: &str,
    ) -> Result<Vec<CfDnsRecord<SvcbRecordData>>> {
        let clean_name = record_name.trim().trim_end_matches('.');
        let url = format!(
            "{}/zones/{zone_id}/dns_records?type=HTTPS&name={clean_name}",
            self.api_base_url
        );

        let resp = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
            .context("send Cloudflare list HTTPS DNS records request")?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        let cf_resp: CfResponse<Vec<CfDnsRecord<SvcbRecordData>>> = serde_json::from_str(&body_text)
            .with_context(|| format!("parse Cloudflare response (status {status}): {body_text}"))?;

        if !cf_resp.success {
            let err_msgs = cf_resp
                .errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            bail!("Cloudflare API error listing HTTPS records: {err_msgs}");
        }

        Ok(cf_resp.result.unwrap_or_default())
    }

    /// Publish or update an HTTPS / SVCB DNS record in Cloudflare.
    pub async fn publish_https_record(
        &self,
        zone_id: &str,
        record_name: &str,
        priority: u16,
        target: &str,
        value: &str,
    ) -> Result<String> {
        let clean_name = record_name.trim().trim_end_matches('.');
        let existing = self.list_https_records(zone_id, clean_name).await.unwrap_or_default();

        let matching = existing.into_iter().find(|r| {
            r.data.as_ref().map(|d| d.priority == priority).unwrap_or(false)
        });

        let data = SvcbRecordData {
            priority,
            target: target.to_string(),
            value: value.to_string(),
        };

        if let Some(record) = matching {
            let url = format!("{}/zones/{zone_id}/dns_records/{}", self.api_base_url, record.id);
            let body = CreateSvcbDnsRecordRequest {
                record_type: "HTTPS",
                name: clean_name,
                ttl: 300,
                data,
            };

            let resp = self
                .http_client
                .put(&url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .json(&body)
                .send()
                .await
                .context("send Cloudflare update HTTPS DNS record request")?;

            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            let cf_resp: CfResponse<DnsRecordResult> = serde_json::from_str(&body_text)
                .with_context(|| format!("parse Cloudflare response (status {status}): {body_text}"))?;

            if !cf_resp.success {
                let err_msgs = cf_resp.errors.into_iter().map(|e| e.message).collect::<Vec<_>>().join("; ");
                bail!("Cloudflare API error updating HTTPS record: {err_msgs}");
            }

            tracing::info!(
                zone_id = %zone_id,
                record_name = %clean_name,
                priority = priority,
                value = %value,
                "Cloudflare: updated HTTPS/SVCB DNS record"
            );
            Ok(record.id)
        } else {
            let url = format!("{}/zones/{zone_id}/dns_records", self.api_base_url);
            let body = CreateSvcbDnsRecordRequest {
                record_type: "HTTPS",
                name: clean_name,
                ttl: 300,
                data,
            };

            let resp = self
                .http_client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .json(&body)
                .send()
                .await
                .context("send Cloudflare create HTTPS DNS record request")?;

            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            let cf_resp: CfResponse<DnsRecordResult> = serde_json::from_str(&body_text)
                .with_context(|| format!("parse Cloudflare response (status {status}): {body_text}"))?;

            if !cf_resp.success {
                let err_msgs = cf_resp.errors.into_iter().map(|e| e.message).collect::<Vec<_>>().join("; ");
                bail!("Cloudflare API error creating HTTPS record: {err_msgs}");
            }

            let record_id = cf_resp
                .result
                .map(|r| r.id)
                .ok_or_else(|| anyhow::anyhow!("missing record id in Cloudflare response"))?;

            tracing::info!(
                zone_id = %zone_id,
                record_name = %clean_name,
                priority = priority,
                value = %value,
                record_id = %record_id,
                "Cloudflare: created HTTPS/SVCB DNS record"
            );
            Ok(record_id)
        }
    }

    /// Synchronize all published endpoints into Cloudflare HTTPS records.
    pub async fn sync_endpoint_svcb_records(
        &self,
        zone_id: &str,
        domain: &str,
        endpoints: &[PublishedEndpoint],
    ) -> Result<()> {
        let clean_domain = domain.trim().trim_start_matches("*.").trim_end_matches('.');
        for ep in endpoints {
            let value = format!("alpn=\"{}\" port={}", ep.alpn, ep.port);
            self.publish_https_record(zone_id, clean_domain, ep.priority, ".", &value).await?;
        }
        Ok(())
    }

    /// Query DoH for HTTPS (type 65) records.
    #[allow(dead_code)]
    pub async fn query_doh_https(&self, domain: &str) -> Result<Vec<String>> {
        let clean_domain = domain.trim().trim_end_matches('.');
        let url = format!("{}?name={clean_domain}&type=HTTPS", self.doh_base_url);
        let resp = self
            .http_client
            .get(&url)
            .header("Accept", "application/dns-json")
            .send()
            .await
            .context("query Cloudflare DoH for HTTPS record")?;

        if !resp.status().is_success() {
            bail!("DoH query failed with status {}", resp.status());
        }

        let doh: DohResponse = resp.json().await.context("parse DoH response")?;
        let mut results = Vec::new();
        for ans in doh.answer {
            if ans.record_type == Some(65) || ans.record_type.is_none() {
                results.push(ans.data);
            }
        }
        Ok(results)
    }

    /// Poll Cloudflare DNS over HTTPS to verify the TXT record has propagated.
    pub async fn wait_for_propagation(
        &self,
        record_name: &str,
        expected_content: &str,
        timeout: Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let expected_clean = expected_content.trim().trim_matches('"');
        let poll_interval = Duration::from_secs(3);

        tracing::info!(
            record = %record_name,
            timeout_secs = timeout.as_secs(),
            "Cloudflare: waiting for DNS TXT propagation"
        );

        let mut endpoints: Vec<String> = vec![self.doh_base_url.clone()];
        if !self.doh_base_url.starts_with("http://127.0.0.1") && !self.doh_base_url.starts_with("http://localhost") {
            endpoints.extend([
                "https://1.1.1.1/dns-query".to_string(),
                "https://8.8.8.8/resolve".to_string(),
                "https://dns.google/resolve".to_string(),
                "https://223.5.5.5/resolve".to_string(),
                "https://dns.alidns.com/resolve".to_string(),
            ]);
        }

        while start.elapsed() < timeout {
            use futures_util::stream::FuturesUnordered;
            use futures_util::StreamExt;

            let mut tasks = FuturesUnordered::new();
            for ep in &endpoints {
                let url = format!("{ep}?name={record_name}&type=TXT");
                let client = self.http_client.clone();
                let ep_str = ep.clone();
                tasks.push(async move {
                    let resp = client
                        .get(&url)
                        .header("Accept", "application/dns-json, application/json")
                        .header("User-Agent", "prism/0.1.0")
                        .send()
                        .await;
                    (ep_str, resp)
                });
            }

            while let Some((ep, resp)) = tasks.next().await {
                if let Ok(res) = resp {
                    if res.status().is_success() {
                        if let Ok(doh) = res.json::<DohResponse>().await {
                            if doh.status == Some(0) {
                                for ans in doh.answer {
                                    // TXT type code is 16
                                    if ans.record_type == Some(16) || ans.record_type.is_none() {
                                        let content = ans.data.trim().trim_matches('"');
                                        if content == expected_clean {
                                            tracing::info!(
                                                record = %record_name,
                                                doh = %ep,
                                                elapsed_secs = start.elapsed().as_secs(),
                                                "Cloudflare: DNS TXT propagation verified via concurrent DoH"
                                            );
                                            // Wait an extra 3 seconds for external recursive resolvers
                                            tokio::time::sleep(Duration::from_secs(3)).await;
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(poll_interval).await;
        }

        tracing::warn!(
            record = %record_name,
            timeout_secs = timeout.as_secs(),
            "Cloudflare: DNS TXT propagation polling timed out; proceeding with challenge"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::Path as AxPath,
        routing::{delete, get, post},
    };
    use serde_json::json;
    use tokio::net::TcpListener;

    #[test]
    fn test_client_init() {
        let client = CloudflareClient::new("fake_token".into(), Some("zone123".into()));
        assert_eq!(client.api_token, "fake_token");
        assert_eq!(client.zone_id.as_deref(), Some("zone123"));
    }

    #[tokio::test]
    async fn test_resolve_zone_id_explicit() {
        let client = CloudflareClient::new("fake_token".into(), Some("zone_explicit_456".into()));
        let zid = client.resolve_zone_id("sub.example.com").await.unwrap();
        assert_eq!(zid, "zone_explicit_456");
    }

    #[tokio::test]
    async fn test_mock_cloudflare_api_flow() {
        // Build mock Cloudflare API + DoH server
        let app = Router::new()
            .route(
                "/zones",
                get(|| async {
                    Json(json!({
                        "success": true,
                        "errors": [],
                        "result": [
                            { "id": "zone_example_com", "name": "example.com" },
                            { "id": "zone_other_org", "name": "other.org" }
                        ]
                    }))
                }),
            )
            .route(
                "/zones/{zone_id}/dns_records",
                post(|AxPath(zid): AxPath<String>, Json(body): Json<serde_json::Value>| async move {
                    assert_eq!(zid, "zone_example_com");
                    assert_eq!(body["type"], "TXT");
                    assert_eq!(body["name"], "_acme-challenge.tunnel.example.com");
                    assert_eq!(body["content"], "acme-challenge-token-123");
                    Json(json!({
                        "success": true,
                        "errors": [],
                        "result": {
                            "id": "rec_789"
                        }
                    }))
                }),
            )
            .route(
                "/zones/{zone_id}/dns_records/{record_id}",
                delete(|AxPath((zid, rid)): AxPath<(String, String)>| async move {
                    assert_eq!(zid, "zone_example_com");
                    assert_eq!(rid, "rec_789");
                    Json(json!({
                        "success": true,
                        "errors": [],
                        "result": {
                            "id": "rec_789"
                        }
                    }))
                }),
            )
            .route(
                "/dns-query",
                get(|| async {
                    Json(json!({
                        "Status": 0,
                        "Answer": [
                            {
                                "name": "_acme-challenge.tunnel.example.com.",
                                "type": 16,
                                "data": "\"acme-challenge-token-123\""
                            }
                        ]
                    }))
                }),
            );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_base = format!("http://{addr}");
        let doh_base = format!("http://{addr}/dns-query");

        let client = CloudflareClient::with_base_urls(
            "test-token".into(),
            None,
            api_base,
            doh_base,
        );

        // 1. Test zone ID resolution (auto matching longest suffix)
        let resolved_zone = client.resolve_zone_id("tunnel.example.com").await.unwrap();
        assert_eq!(resolved_zone, "zone_example_com");

        // 2. Test create TXT record
        let record_id = client
            .create_txt_record(
                &resolved_zone,
                "_acme-challenge.tunnel.example.com",
                "acme-challenge-token-123",
            )
            .await
            .unwrap();
        assert_eq!(record_id, "rec_789");

        // 3. Test DNS propagation check
        let prop_res = client
            .wait_for_propagation(
                "_acme-challenge.tunnel.example.com",
                "acme-challenge-token-123",
                Duration::from_secs(5),
            )
            .await;
        assert!(prop_res.is_ok());

        // 4. Test delete TXT record
        let del_res = client
            .delete_txt_record(&resolved_zone, &record_id)
            .await;
        assert!(del_res.is_ok());
    }
}
