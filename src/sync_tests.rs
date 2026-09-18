use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Exchange {
    method: &'static str,
    path: &'static str,
    body: Option<Value>,
    result: Value,
}

async fn mock(exchanges: Vec<Exchange>) -> (Cloudflare, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/client/v4", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for exchange in exchanges {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            assert_eq!(
                headers.lines().next().unwrap(),
                format!("{} {} HTTP/1.1", exchange.method, exchange.path)
            );
            if let Some(expected) = exchange.body {
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes[header_end..]).unwrap(),
                    expected
                );
            }
            let body = json!({"success":true,"errors":[],"result":exchange.result,
                "result_info":{"page":1,"total_pages":1}})
            .to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        }
    });
    (Cloudflare::for_test(&url), task)
}

fn remote(id: &str, content: &str) -> Value {
    json!({"id":id,"name":"host.example.com","type":"A","content":content,"ttl":3600,"proxied":false})
}

fn zones() -> Exchange {
    Exchange {
        method: "GET",
        path: "/client/v4/zones?page=1&per_page=100",
        body: None,
        result: json!([{"id":"zone-id","name":"example.com"}]),
    }
}

fn listing(records: Value) -> Exchange {
    Exchange {
        method: "GET",
        path: "/client/v4/zones/zone-id/dns_records?page=1&per_page=100",
        body: None,
        result: records,
    }
}

#[tokio::test]
async fn reconciliation_integration() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let url = match std::env::var("DDNSER_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => { eprintln!("DDNSER_TEST_DATABASE_URL not set, skipping"); return; },
        };
        let storage = Arc::new(Storage::connect(&url).await.unwrap());
        let input = crate::storage::RecordInput {
            fqdn: "host.example.com".into(), record_type: "A".into(), value: Some("192.0.2.2".into()),
            ttl: 3600, priority: None, weight: None, port: None,
        };
        let request = SyncRequest { source: "record-created", myip: None, respond: None };

        // An identical existing record is automatically reused, with no write.
        let record = storage.create(&input).await.unwrap();
        let (cloudflare, server) = mock(vec![zones(), listing(json!([remote("existing-id", "192.0.2.2")]))]).await;
        let outcome = run_sync(&storage, &cloudflare, &request).await;
        assert!(outcome.ok());
        assert_eq!(outcome.unchanged, 1);
        assert_eq!(storage.get(record.id).await.unwrap().unwrap().provider_record_id.as_deref(), Some("existing-id"));
        server.await.unwrap();

        // Future edits use that exact ID, replacing rather than duplicating it.
        let changed = crate::storage::RecordInput { value: Some("192.0.2.3".into()), ..input.clone() };
        storage.update(record.id, &changed).await.unwrap();
        let (cloudflare, server) = mock(vec![zones(), listing(json!([remote("existing-id", "192.0.2.2")])),
            Exchange { method: "PUT", path: "/client/v4/zones/zone-id/dns_records/existing-id",
                body: Some(json!({"name":"host.example.com","type":"A","content":"192.0.2.3","ttl":3600,"proxied":false})),
                result: remote("existing-id", "192.0.2.3") },
        ]).await;
        assert_eq!(run_sync(&storage, &cloudflare, &SyncRequest { source: "record-updated", myip: None, respond: None }).await.updated, 1);
        server.await.unwrap();
        storage.delete(record.id).await.unwrap();

        // A different remote record remains untouched; ddnser creates its local desired record.
        let record = storage.create(&input).await.unwrap();
        let (cloudflare, server) = mock(vec![zones(), listing(json!([remote("other-id", "192.0.2.1")])),
            Exchange { method: "POST", path: "/client/v4/zones/zone-id/dns_records",
                body: Some(json!({"name":"host.example.com","type":"A","content":"192.0.2.2","ttl":3600,"proxied":false})),
                result: remote("created-id", "192.0.2.2") },
        ]).await;
        assert_eq!(run_sync(&storage, &cloudflare, &request).await.created, 1);
        assert_eq!(storage.get(record.id).await.unwrap().unwrap().provider_record_id.as_deref(), Some("created-id"));
        server.await.unwrap();

        let dynamic = crate::storage::RecordInput { value: None, ..input.clone() };
        let row = storage.create(&dynamic).await.unwrap();
        assert!(storage.create(&dynamic).await.is_err());
        storage.delete(row.id).await.unwrap();
    }).await.unwrap();
}
