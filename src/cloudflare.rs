use std::time::Duration;

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const API_URL: &str = "https://api.cloudflare.com/client/v4";

pub struct Cloudflare {
    client: Client,
    token: String,
    base_url: reqwest::Url,
}

#[derive(Debug, thiserror::Error)]
pub enum CloudflareError {
    #[error("Cloudflare request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Cloudflare rejected request (HTTP {status}, error codes {codes:?})")]
    Api { status: u16, codes: Vec<u64> },
    #[error("invalid Cloudflare response")]
    InvalidResponse,
}

#[derive(Debug, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub content: String,
    pub ttl: u32,
    #[serde(default)]
    pub proxied: bool,
    pub priority: Option<u16>,
    pub data: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    result: Option<T>,
    errors: Vec<ApiError>,
    result_info: Option<PageInfo>,
}

#[derive(Deserialize)]
struct ApiError {
    code: u64,
}

#[derive(Deserialize)]
struct PageInfo {
    page: u32,
    total_pages: u32,
}

/// Cloudflare expects full record names and structured SRV and CAA data.
#[derive(Serialize)]
pub struct Record<'a> {
    pub name: &'a str,
    #[serde(rename = "type")]
    pub record_type: &'a str,
    pub content: &'a str,
    pub ttl: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<RecordData<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<&'a str>,
}

#[derive(Serialize)]
pub struct SrvData<'a> {
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: &'a str,
}

#[derive(Serialize)]
pub struct CaaData<'a> {
    pub flags: u8,
    pub tag: &'a str,
    pub value: &'a str,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum RecordData<'a> {
    Srv(SrvData<'a>),
    Caa(CaaData<'a>),
}

#[derive(Serialize)]
struct RecordBody<'a, 'b> {
    name: &'b str,
    #[serde(rename = "type")]
    record_type: &'b str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'b str>,
    ttl: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<&'a RecordData<'b>>,
    proxied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<&'b str>,
}

impl Cloudflare {
    pub fn new(token: String) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()?,
            token,
            base_url: reqwest::Url::parse(API_URL).expect("constant API URL"),
        })
    }

    #[cfg(test)]
    pub fn for_test(url: &str) -> Self {
        let base_url = reqwest::Url::parse(url).unwrap();
        assert_eq!(base_url.host_str(), Some("127.0.0.1"));
        Self {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            token: "not-a-real-token".into(),
            base_url,
        }
    }

    fn url(&self, path: &[&str]) -> reqwest::Url {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("HTTP URL supports path segments")
            .extend(path);
        url
    }

    pub async fn get_record(
        &self,
        zone_id: &str,
        record_id: &str,
    ) -> Result<DnsRecord, CloudflareError> {
        let request = self
            .client
            .get(self.url(&["zones", zone_id, "dns_records", record_id]))
            .bearer_auth(&self.token)
            .build()?;
        Ok(self.execute(request).await?.0)
    }

    pub async fn list_zones(&self) -> Result<Vec<Zone>, CloudflareError> {
        self.list(&["zones"]).await
    }

    pub async fn list_records(&self, zone_id: &str) -> Result<Vec<DnsRecord>, CloudflareError> {
        self.list(&["zones", zone_id, "dns_records"]).await
    }

    async fn list<T: DeserializeOwned>(&self, path: &[&str]) -> Result<Vec<T>, CloudflareError> {
        let mut records = Vec::new();
        let mut page = 1;
        loop {
            let request = self
                .client
                .get(self.url(path))
                .bearer_auth(&self.token)
                .query(&[("page", page), ("per_page", 100)])
                .build()?;
            let (mut result, info) = self.execute::<Vec<T>>(request).await?;
            let info = info.ok_or(CloudflareError::InvalidResponse)?;
            if info.page != page || (info.total_pages == 0 && !result.is_empty()) {
                return Err(CloudflareError::InvalidResponse);
            }
            records.append(&mut result);
            if page >= info.total_pages {
                return Ok(records);
            }
            page = page
                .checked_add(1)
                .ok_or(CloudflareError::InvalidResponse)?;
        }
    }

    pub async fn create_record(
        &self,
        zone_id: &str,
        record: &Record<'_>,
    ) -> Result<DnsRecord, CloudflareError> {
        Ok(self.execute(self.create_request(zone_id, record)?).await?.0)
    }

    pub async fn update_record(
        &self,
        zone_id: &str,
        record_id: &str,
        record: &Record<'_>,
    ) -> Result<DnsRecord, CloudflareError> {
        Ok(self
            .execute(self.update_request(zone_id, record_id, record)?)
            .await?
            .0)
    }

    pub async fn delete_record(
        &self,
        zone_id: &str,
        record_id: &str,
    ) -> Result<(), CloudflareError> {
        let request = self
            .client
            .delete(self.url(&["zones", zone_id, "dns_records", record_id]))
            .bearer_auth(&self.token)
            .build()?;
        self.execute::<serde_json::Value>(request).await?;
        Ok(())
    }

    async fn execute<T: DeserializeOwned>(
        &self,
        request: Request,
    ) -> Result<(T, Option<PageInfo>), CloudflareError> {
        let response = self.client.execute(request).await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        decode(status.as_u16(), &bytes)
    }

    fn create_request(
        &self,
        zone_id: &str,
        record: &Record<'_>,
    ) -> Result<Request, reqwest::Error> {
        self.record_request(Method::POST, zone_id, None, record)
    }

    fn update_request(
        &self,
        zone_id: &str,
        record_id: &str,
        record: &Record<'_>,
    ) -> Result<Request, reqwest::Error> {
        self.record_request(Method::PUT, zone_id, Some(record_id), record)
    }

    fn record_request(
        &self,
        method: Method,
        zone_id: &str,
        record_id: Option<&str>,
        record: &Record<'_>,
    ) -> Result<Request, reqwest::Error> {
        let mut path = vec!["zones", zone_id, "dns_records"];
        if let Some(id) = record_id {
            path.push(id);
        }
        let url = self.url(&path);
        self.client
            .request(method, url)
            .bearer_auth(&self.token)
            .json(&RecordBody {
                name: record.name,
                record_type: record.record_type,
                content: record.data.is_none().then_some(record.content),
                ttl: record.ttl,
                priority: record.priority,
                data: record.data.as_ref(),
                proxied: false,
                comment: record.comment,
            })
            .build()
    }
}

fn decode<T: DeserializeOwned>(
    status: u16,
    bytes: &[u8],
) -> Result<(T, Option<PageInfo>), CloudflareError> {
    let envelope: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|_| CloudflareError::InvalidResponse)?;
    if !(200..300).contains(&status) || !envelope.success || !envelope.errors.is_empty() {
        return Err(CloudflareError::Api {
            status,
            codes: envelope
                .errors
                .into_iter()
                .map(|error| error.code)
                .collect(),
        });
    }
    let result = envelope.result.ok_or(CloudflareError::InvalidResponse)?;
    let result = serde_json::from_value(result).map_err(|_| CloudflareError::InvalidResponse)?;
    Ok((result, envelope.result_info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn client() -> Cloudflare {
        Cloudflare::new("not-a-real-token".into()).unwrap()
    }

    fn record() -> Record<'static> {
        Record {
            name: "host.example.com",
            record_type: "A",
            content: "192.0.2.1",
            ttl: 1,
            priority: None,
            data: None,
            comment: None,
        }
    }

    fn body(request: &Request) -> Value {
        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap()
    }

    #[test]
    fn create_uses_zone_id_fqdn_and_unproxied_content() {
        let request = client().create_request("zone-id", &record()).unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request.url().as_str(),
            "https://api.cloudflare.com/client/v4/zones/zone-id/dns_records"
        );
        assert!(request.headers()[reqwest::header::AUTHORIZATION].is_sensitive());
        assert_eq!(
            body(&request),
            json!({
                "name": "host.example.com",
                "type": "A",
                "content": "192.0.2.1",
                "ttl": 1,
                "proxied": false
            })
        );
    }

    #[test]
    fn update_addresses_the_specific_record() {
        let request = client()
            .update_request("zone-id", "record-id", &record())
            .unwrap();
        assert_eq!(request.method(), Method::PUT);
        assert_eq!(
            request.url().as_str(),
            "https://api.cloudflare.com/client/v4/zones/zone-id/dns_records/record-id"
        );
    }

    #[test]
    fn rejects_api_failure_even_with_http_success() {
        let bytes = br#"{"success":false,"result":null,"errors":[{"code":10000,"message":"untrusted response text"}]}"#;
        let error = decode::<Zone>(200, bytes).err().unwrap();
        assert!(matches!(error, CloudflareError::Api { status: 200, .. }));
        assert!(!error.to_string().contains("untrusted response text"));
    }

    #[test]
    fn rejects_http_failure_even_with_success_envelope() {
        let bytes = br#"{"success":true,"result":[],"errors":[]}"#;
        assert!(matches!(
            decode::<Vec<Zone>>(403, bytes),
            Err(CloudflareError::Api { status: 403, .. })
        ));
    }

    #[test]
    fn decodes_zone_pagination() {
        let bytes = br#"{"success":true,"result":[{"id":"zone-id","name":"example.com"}],"errors":[],"result_info":{"page":1,"total_pages":2}}"#;
        let (zones, info) = decode::<Vec<Zone>>(200, bytes).unwrap();
        assert_eq!(zones[0].id, "zone-id");
        assert_eq!(zones[0].name, "example.com");
        let info = info.unwrap();
        assert_eq!(info.page, 1);
        assert_eq!(info.total_pages, 2);
    }

    #[test]
    fn rejects_missing_result_and_invalid_json() {
        assert!(decode::<Zone>(200, br#"{"success":true,"result":null,"errors":[]}"#).is_err());
        assert!(decode::<Zone>(502, b"not JSON").is_err());
    }

    #[test]
    fn mx_includes_priority() {
        let record = Record {
            name: "example.com",
            record_type: "MX",
            content: "mail.example.com",
            priority: Some(10),
            ..record()
        };
        let request = client().create_request("zone-id", &record).unwrap();
        assert_eq!(body(&request)["priority"], 10);
        assert_eq!(body(&request)["content"], "mail.example.com");
    }

    #[test]
    fn caa_uses_structured_data() {
        let record = Record {
            name: "example.com",
            record_type: "CAA",
            content: "0 issue letsencrypt.org",
            data: Some(RecordData::Caa(CaaData {
                flags: 0,
                tag: "issue",
                value: "letsencrypt.org",
            })),
            ..record()
        };
        let request = client().create_request("zone-id", &record).unwrap();
        assert!(body(&request).get("content").is_none());
        assert_eq!(
            body(&request)["data"],
            json!({
                "flags": 0,
                "tag": "issue",
                "value": "letsencrypt.org"
            })
        );
    }

    #[test]
    fn srv_preserves_weight_port_and_target() {
        let record = Record {
            name: "_sip._tcp.example.com",
            record_type: "SRV",
            content: "10 20 5061 sip.example.com",
            data: Some(RecordData::Srv(SrvData {
                priority: 10,
                weight: 20,
                port: 5061,
                target: "sip.example.com",
            })),
            ..record()
        };
        let request = client().create_request("zone-id", &record).unwrap();
        assert!(body(&request).get("content").is_none());
        assert_eq!(
            body(&request)["data"],
            json!({
                "priority": 10,
                "weight": 20,
                "port": 5061,
                "target": "sip.example.com"
            })
        );
    }
}
