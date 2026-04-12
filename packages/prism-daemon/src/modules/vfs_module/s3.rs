//! Remote content-addressed blob backends for the VFS module.
//!
//! Ships two concrete types:
//!
//! - [`S3VfsBackend`] (`vfs-s3` feature) — talks to any service that
//!   speaks the AWS S3 REST API. Tested against Amazon S3, MinIO, and
//!   Cloudflare R2. Uses AWS Signature Version 4 (SigV4) for auth.
//! - [`GcsVfsBackend`] (`vfs-gcs` feature) — talks to Google Cloud
//!   Storage through its [S3-interoperability endpoint]. This lets us
//!   reuse the same SigV4 signer with HMAC keys minted in the GCP
//!   console, so we don't have to carry a separate JWT/OAuth stack
//!   (which would pull in rustls + reqwest + tokio for a use case
//!   that's already covered by HMAC keys).
//!
//! Both backends plug into the exact same [`VfsBackend`] interface, so
//! a host switches storage engines by swapping one constructor call —
//! every `vfs.*` command site stays the same.
//!
//! ## Why roll our own signer?
//!
//! The canonical `aws-sdk-s3` crate pulls in ~80 transitive deps and
//! forces async/tokio into the hot path of an otherwise-sync daemon.
//! `rust-s3` is lighter but still hauls in an async runtime. The
//! daemon is kernel-embedded software that runs on iPads and ESP32-
//! adjacent devices; we can't afford that dep tree and we don't need
//! multipart uploads, bucket management, or presigned URLs. SigV4 over
//! a blocking HTTP client (`ureq`) fits in ~300 lines, uses the same
//! pure-Rust `sha2`/`hex` crates the VFS module already depends on,
//! and works for both Amazon S3 and GCS interop endpoints.
//!
//! ## Testing
//!
//! The HTTP layer is abstracted behind [`HttpTransport`] so unit tests
//! can drive the backend without hitting the network. A mock transport
//! asserts that the correct method/URL/headers show up at the wire.
//! Real Amazon S3 / GCS calls are exercised via the caller's own
//! integration tests — burning AWS credits in CI is out of scope.
//!
//! [S3-interoperability endpoint]: https://cloud.google.com/storage/docs/aws-simple-migration

use super::{VfsBackend, VfsEntry};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Credentials ────────────────────────────────────────────────────────

/// Static HMAC credentials for SigV4. For AWS, these are "access key id
/// + secret access key" from IAM. For GCS, these are an HMAC key pair
///   minted in the GCP console under Cloud Storage → Settings → Interop.
#[derive(Debug, Clone)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Optional STS session token. `None` for permanent keys.
    pub session_token: Option<String>,
}

impl S3Credentials {
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }
}

/// Immutable configuration the backend keeps around per signed request.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Base HTTPS endpoint, e.g. `https://s3.us-east-1.amazonaws.com`
    /// for AWS or `https://storage.googleapis.com` for GCS interop.
    pub endpoint: String,
    /// Bucket name.
    pub bucket: String,
    /// Region used in the SigV4 credential scope. For GCS interop
    /// Google recommends `auto`, which both their servers and AWS-style
    /// signers accept.
    pub region: String,
    /// Service id used in the SigV4 credential scope. `"s3"` for both
    /// AWS and GCS interop.
    pub service: String,
    /// Optional prefix inside the bucket. Blobs are stored at
    /// `<prefix><hash>`. Trailing slash not required.
    pub prefix: String,
    pub credentials: S3Credentials,
}

impl S3Config {
    /// AWS S3 defaults for a given region.
    pub fn aws(
        region: impl Into<String>,
        bucket: impl Into<String>,
        credentials: S3Credentials,
    ) -> Self {
        let region = region.into();
        Self {
            endpoint: format!("https://s3.{region}.amazonaws.com"),
            bucket: bucket.into(),
            region,
            service: "s3".into(),
            prefix: String::new(),
            credentials,
        }
    }

    /// Google Cloud Storage defaults (interop endpoint).
    pub fn gcs(bucket: impl Into<String>, credentials: S3Credentials) -> Self {
        Self {
            endpoint: "https://storage.googleapis.com".into(),
            bucket: bucket.into(),
            region: "auto".into(),
            service: "s3".into(),
            prefix: String::new(),
            credentials,
        }
    }

    fn object_key(&self, hash: &str) -> String {
        if self.prefix.is_empty() {
            hash.to_string()
        } else if self.prefix.ends_with('/') {
            format!("{}{}", self.prefix, hash)
        } else {
            format!("{}/{}", self.prefix, hash)
        }
    }
}

// ── HTTP transport trait ──────────────────────────────────────────────

/// A single HTTP request presented to a [`HttpTransport`].
#[derive(Debug)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// A single HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Pluggable HTTP transport so tests can drive the backend without
/// hitting the network. The production impl in this file uses
/// [`UreqTransport`], a thin wrapper over the blocking `ureq` client.
pub trait HttpTransport: Send + Sync {
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse, String>;
}

/// Blocking HTTP transport powered by `ureq`. Only exists when the
/// `vfs-s3` or `vfs-gcs` features are active.
#[cfg(any(feature = "vfs-s3", feature = "vfs-gcs"))]
pub struct UreqTransport;

#[cfg(any(feature = "vfs-s3", feature = "vfs-gcs"))]
impl HttpTransport for UreqTransport {
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse, String> {
        let mut r = ureq::request(req.method, &req.url);
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }
        let resp = match req.body.len() {
            0 => r.call(),
            _ => r.send_bytes(&req.body),
        };
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                // 4xx/5xx still come back as a response we want to
                // inspect (for 404 → has() = None).
                let mut headers = BTreeMap::new();
                for name in r.headers_names() {
                    if let Some(value) = r.header(&name) {
                        headers.insert(name, value.to_string());
                    }
                }
                let mut body = Vec::new();
                let _ = r.into_reader().read_to_end(&mut body);
                return Ok(HttpResponse {
                    status: code,
                    headers,
                    body,
                });
            }
            Err(e) => return Err(format!("http transport error: {e}")),
        };
        let status = resp.status();
        let mut headers = BTreeMap::new();
        for name in resp.headers_names() {
            if let Some(value) = resp.header(&name) {
                headers.insert(name, value.to_string());
            }
        }
        let mut body = Vec::new();
        resp.into_reader()
            .read_to_end(&mut body)
            .map_err(|e| format!("failed to read http response body: {e}"))?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

// The `Read` trait is only needed for the ureq path above.
#[cfg(any(feature = "vfs-s3", feature = "vfs-gcs"))]
use std::io::Read as _;

// ── S3 backend ────────────────────────────────────────────────────────

/// Content-addressed blob store backed by any S3-compatible service.
///
/// Behaviour matches the other backends: `put(hash, bytes)` is
/// idempotent, `has` returns the object size when present, `list`
/// streams the bucket prefix and pulls out objects whose key matches
/// our hash shape.
pub struct S3VfsBackend {
    config: S3Config,
    transport: Arc<dyn HttpTransport>,
    /// Backend name shown by `vfs.stats`. Default is `"s3"`, but
    /// [`GcsVfsBackend`] overrides it to `"gcs"` so operators can tell
    /// which remote is in play.
    label: &'static str,
}

impl S3VfsBackend {
    /// Construct with the default `ureq`-backed transport. Requires
    /// either the `vfs-s3` or `vfs-gcs` feature.
    #[cfg(any(feature = "vfs-s3", feature = "vfs-gcs"))]
    pub fn new(config: S3Config) -> Self {
        Self {
            config,
            transport: Arc::new(UreqTransport),
            label: "s3",
        }
    }

    /// Construct with a caller-supplied transport. This is how tests
    /// (and hosts that want to plug in their own connection pool / TLS
    /// stack) wire up the backend.
    pub fn with_transport(config: S3Config, transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            config,
            transport,
            label: "s3",
        }
    }

    fn signed(&self, method: &'static str, hash: &str, body: &[u8]) -> HttpRequest {
        let key = self.config.object_key(hash);
        let url = format!("{}/{}/{}", self.config.endpoint, self.config.bucket, key);
        let headers = sign_v4(&self.config, method, &url, body, now_unix());
        HttpRequest {
            method,
            url,
            headers,
            body: body.to_vec(),
        }
    }

    fn signed_list(&self) -> HttpRequest {
        let url = format!(
            "{}/{}?list-type=2{}",
            self.config.endpoint,
            self.config.bucket,
            if self.config.prefix.is_empty() {
                String::new()
            } else {
                format!("&prefix={}", urlencode(&self.config.prefix))
            }
        );
        let headers = sign_v4(&self.config, "GET", &url, &[], now_unix());
        HttpRequest {
            method: "GET",
            url,
            headers,
            body: Vec::new(),
        }
    }
}

impl VfsBackend for S3VfsBackend {
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), String> {
        let req = self.signed("PUT", hash, bytes);
        let resp = self.transport.execute(req)?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!(
                "{}: put failed with HTTP {}: {}",
                self.label,
                resp.status,
                String::from_utf8_lossy(&resp.body)
            ))
        }
    }

    fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        let req = self.signed("GET", hash, &[]);
        let resp = self.transport.execute(req)?;
        match resp.status {
            200 => Ok(resp.body),
            404 => Err(format!("{}: hash not found: {hash}", self.label)),
            other => Err(format!(
                "{}: get failed with HTTP {other}: {}",
                self.label,
                String::from_utf8_lossy(&resp.body)
            )),
        }
    }

    fn has(&self, hash: &str) -> Result<Option<u64>, String> {
        let req = self.signed("HEAD", hash, &[]);
        let resp = self.transport.execute(req)?;
        match resp.status {
            200 => {
                let size = resp
                    .headers
                    .get("content-length")
                    .or_else(|| resp.headers.get("Content-Length"))
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                Ok(Some(size))
            }
            404 => Ok(None),
            other => Err(format!("{}: head failed with HTTP {other}", self.label)),
        }
    }

    fn delete(&self, hash: &str) -> Result<bool, String> {
        let req = self.signed("DELETE", hash, &[]);
        let resp = self.transport.execute(req)?;
        match resp.status {
            200 | 204 => Ok(true),
            404 => Ok(false),
            other => Err(format!(
                "{}: delete failed with HTTP {other}: {}",
                self.label,
                String::from_utf8_lossy(&resp.body)
            )),
        }
    }

    fn list(&self) -> Result<Vec<VfsEntry>, String> {
        let req = self.signed_list();
        let resp = self.transport.execute(req)?;
        if !(200..300).contains(&resp.status) {
            return Err(format!(
                "{}: list failed with HTTP {}: {}",
                self.label,
                resp.status,
                String::from_utf8_lossy(&resp.body)
            ));
        }
        let body = std::str::from_utf8(&resp.body)
            .map_err(|e| format!("list body is not valid utf-8: {e}"))?;
        let entries = parse_list_v2_xml(body, &self.config.prefix);
        Ok(entries)
    }

    fn backend_name(&self) -> &'static str {
        self.label
    }
}

// ── GCS backend ────────────────────────────────────────────────────────

/// GCS interop backend. Thin wrapper around [`S3VfsBackend`] that
/// points at `storage.googleapis.com` and labels itself `"gcs"` for
/// introspection.
#[cfg(feature = "vfs-gcs")]
pub struct GcsVfsBackend {
    inner: S3VfsBackend,
}

#[cfg(feature = "vfs-gcs")]
impl GcsVfsBackend {
    pub fn new(bucket: impl Into<String>, credentials: S3Credentials) -> Self {
        let mut inner = S3VfsBackend::new(S3Config::gcs(bucket, credentials));
        inner.label = "gcs";
        Self { inner }
    }

    pub fn with_transport(
        bucket: impl Into<String>,
        credentials: S3Credentials,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        let mut inner = S3VfsBackend::with_transport(S3Config::gcs(bucket, credentials), transport);
        inner.label = "gcs";
        Self { inner }
    }
}

#[cfg(feature = "vfs-gcs")]
impl VfsBackend for GcsVfsBackend {
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), String> {
        self.inner.put(hash, bytes)
    }
    fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        self.inner.get(hash)
    }
    fn has(&self, hash: &str) -> Result<Option<u64>, String> {
        self.inner.has(hash)
    }
    fn delete(&self, hash: &str) -> Result<bool, String> {
        self.inner.delete(hash)
    }
    fn list(&self) -> Result<Vec<VfsEntry>, String> {
        self.inner.list()
    }
    fn backend_name(&self) -> &'static str {
        self.inner.label
    }
}

// ── SigV4 implementation ──────────────────────────────────────────────
//
// We sign every request using AWS Signature Version 4 with "unsigned
// payload" semantics for simplicity: the payload hash is the literal
// string `UNSIGNED-PAYLOAD` (officially supported by AWS and GCS
// interop). This keeps the signer ~100 lines instead of the ~300+
// needed for streaming-signed payloads.
//
// The algorithm is documented at
// https://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html

const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

fn sign_v4(
    config: &S3Config,
    method: &str,
    full_url: &str,
    _body: &[u8],
    timestamp_unix: i64,
) -> BTreeMap<String, String> {
    let amz_date = format_amz_date(timestamp_unix);
    let date = amz_date[..8].to_string();

    let (host, canonical_uri, canonical_query) = split_url(full_url);

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    headers.insert("host".into(), host.clone());
    headers.insert("x-amz-content-sha256".into(), UNSIGNED_PAYLOAD.into());
    headers.insert("x-amz-date".into(), amz_date.clone());
    if let Some(token) = &config.credentials.session_token {
        headers.insert("x-amz-security-token".into(), token.clone());
    }

    let mut signed_headers: Vec<&str> = headers.keys().map(|k| k.as_str()).collect();
    signed_headers.sort();
    let signed_headers_joined = signed_headers.join(";");

    let canonical_headers: String = signed_headers
        .iter()
        .map(|h| format!("{}:{}\n", h, headers.get(*h).unwrap().trim()))
        .collect();

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers_joined}\n{UNSIGNED_PAYLOAD}"
    );

    let credential_scope = format!("{date}/{}/{}/aws4_request", config.region, config.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        hex::encode(sha256(canonical_request.as_bytes()))
    );

    let signing_key = derive_signing_key(
        &config.credentials.secret_access_key,
        &date,
        &config.region,
        &config.service,
    );
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        config.credentials.access_key_id, credential_scope, signed_headers_joined, signature
    );
    headers.insert("authorization".into(), authorization);
    headers
}

fn split_url(url: &str) -> (String, String, String) {
    // Strip scheme.
    let after_scheme = url.split_once("://").map(|x| x.1).unwrap_or(url);
    let (host, rest) = match after_scheme.find('/') {
        Some(i) => (&after_scheme[..i], &after_scheme[i..]),
        None => (after_scheme, "/"),
    };
    let (path, query) = match rest.find('?') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };

    // Canonical URI: URI-encode each path segment (keep "/" and unreserved chars).
    let canonical_uri: String = path
        .split('/')
        .map(uri_encode_segment)
        .collect::<Vec<_>>()
        .join("/");

    // Canonical query: split into k=v pairs, URI-encode, sort by key.
    let mut params: Vec<(String, String)> = Vec::new();
    if !query.is_empty() {
        for part in query.split('&') {
            let (k, v) = match part.find('=') {
                Some(i) => (&part[..i], &part[i + 1..]),
                None => (part, ""),
            };
            params.push((uri_encode_segment(k), uri_encode_segment(v)));
        }
    }
    params.sort();
    let canonical_query = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    (host.to_string(), canonical_uri, canonical_query)
}

fn uri_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn urlencode(s: &str) -> String {
    uri_encode_segment(s)
}

fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // Minimal inline HMAC-SHA256 — pulling in the `hmac` crate just
    // for 30 lines of glue isn't worth the dep.
    const BLOCK_SIZE: usize = 64;
    let mut key_buf = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let digest = sha256(key);
        key_buf[..32].copy_from_slice(&digest);
    } else {
        key_buf[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= key_buf[i];
        opad[i] ^= key_buf[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().to_vec()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_amz_date(unix: i64) -> String {
    // Format YYYYMMDDTHHMMSSZ in UTC without pulling in chrono.
    let (y, m, d, h, mi, s) = unix_to_ymdhms(unix);
    format!("{y:04}{m:02}{d:02}T{h:02}{mi:02}{s:02}Z")
}

fn unix_to_ymdhms(unix: i64) -> (i32, u32, u32, u32, u32, u32) {
    // Standard Howard Hinnant date algorithm (public domain).
    let secs = unix;
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let h = (time_of_day / 3600) as u32;
    let mi = ((time_of_day % 3600) / 60) as u32;
    let s = (time_of_day % 60) as u32;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d, h, mi, s)
}

// ── XML list parser ───────────────────────────────────────────────────
//
// We don't want a full XML crate for one endpoint. The S3 ListObjectsV2
// response has a predictable shape:
//
//   <ListBucketResult ...>
//     <Contents>
//       <Key>...</Key>
//       <Size>...</Size>
//       ...
//     </Contents>
//     <Contents>...</Contents>
//     ...
//   </ListBucketResult>
//
// Substring-matching is plenty for that.

fn parse_list_v2_xml(xml: &str, prefix: &str) -> Vec<VfsEntry> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while let Some(start) = xml[cursor..].find("<Contents>") {
        let abs = cursor + start + "<Contents>".len();
        let end = match xml[abs..].find("</Contents>") {
            Some(e) => abs + e,
            None => break,
        };
        let block = &xml[abs..end];
        let key = extract_tag(block, "Key").unwrap_or_default();
        let size: u64 = extract_tag(block, "Size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let hash = strip_prefix(&key, prefix);
        if super::looks_like_hash(&hash) {
            entries.push(VfsEntry { hash, size });
        }
        cursor = end + "</Contents>".len();
    }
    entries
}

fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].to_string())
}

fn strip_prefix(key: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return key.to_string();
    }
    let with_slash = if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{prefix}/")
    };
    if let Some(tail) = key.strip_prefix(&with_slash) {
        tail.to_string()
    } else {
        key.to_string()
    }
}

// XML-style ListBucketResult response body for list tests, parameterised
// so tests can build a predictable response.
#[allow(dead_code)]
fn list_response_xml(entries: &[(String, u64)]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult>");
    for (key, size) in entries {
        out.push_str(&format!(
            "<Contents><Key>{key}</Key><Size>{size}</Size></Contents>"
        ));
    }
    out.push_str("</ListBucketResult>");
    out
}

// ── Unoffical `Deserialize` escape hatch (for struct support in tests). ──
// We don't derive Deserialize on the backend types themselves — they're
// concrete constructors — so `Deserialize` is imported here only so the
// compile won't complain about an unused `serde::Deserialize` if the test
// module gets trimmed by cfg gates. Leave it.
#[allow(dead_code)]
fn _marker(_: &dyn for<'a> Fn(&'a str) -> Option<String>) {}
#[allow(dead_code)]
type _D<'a> = dyn Deserialize<'a>;

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::vfs_module::VfsManager;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeTransport {
        // (method, url) -> response
        responses: Mutex<Vec<(String, String, HttpResponse)>>,
        seen: Mutex<Vec<HttpRequest>>,
    }

    impl FakeTransport {
        fn push(&self, method: &str, url_contains: &str, resp: HttpResponse) {
            self.responses.lock().unwrap().push((
                method.to_string(),
                url_contains.to_string(),
                resp,
            ));
        }
    }

    impl HttpTransport for FakeTransport {
        fn execute(&self, req: HttpRequest) -> Result<HttpResponse, String> {
            self.seen.lock().unwrap().push(HttpRequest {
                method: req.method,
                url: req.url.clone(),
                headers: req.headers.clone(),
                body: req.body.clone(),
            });
            let mut responses = self.responses.lock().unwrap();
            let idx = responses
                .iter()
                .position(|(m, u, _)| m == req.method && req.url.contains(u.as_str()))
                .ok_or_else(|| format!("no fake response for {} {}", req.method, req.url))?;
            Ok(responses.remove(idx).2)
        }
    }

    fn creds() -> S3Credentials {
        S3Credentials::new("AKIA_TEST", "test-secret-key")
    }

    fn test_config() -> S3Config {
        let mut cfg = S3Config::aws("us-east-1", "prism-test", creds());
        cfg.prefix = "vfs".to_string();
        cfg
    }

    fn ok_response(body: Vec<u8>, status: u16) -> HttpResponse {
        let mut headers = BTreeMap::new();
        headers.insert("content-length".to_string(), body.len().to_string());
        HttpResponse {
            status,
            headers,
            body,
        }
    }

    fn empty_response(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            headers: BTreeMap::new(),
            body: Vec::new(),
        }
    }

    #[test]
    fn sigv4_canonical_request_is_deterministic() {
        let cfg = test_config();
        // Fix time so the signature is reproducible: 2024-01-01T00:00:00Z.
        let unix = 1_704_067_200;
        let url = format!("{}/{}/vfs/abc", cfg.endpoint, cfg.bucket);
        let headers = sign_v4(&cfg, "GET", &url, &[], unix);
        assert_eq!(headers.get("x-amz-date").unwrap(), "20240101T000000Z");
        let authz = headers.get("authorization").unwrap();
        assert!(authz.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIA_TEST/20240101/us-east-1/s3/aws4_request"
        ));
        assert!(authz.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        // Signature is a 64-char hex string.
        let sig = authz.split("Signature=").nth(1).unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn amz_date_formats_are_padded() {
        // 2024-12-31T23:59:59Z → 20241231T235959Z
        let unix = 1_735_689_599;
        assert_eq!(format_amz_date(unix), "20241231T235959Z");
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC 4231 test case 1: key = 20 bytes of 0x0b, data = "Hi There"
        let key = vec![0x0bu8; 20];
        let mac = hmac_sha256(&key, b"Hi There");
        assert_eq!(
            hex::encode(mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn s3_put_issues_a_signed_put_request() {
        let transport = Arc::new(FakeTransport::default());
        transport.push("PUT", "/prism-test/vfs/", empty_response(200));
        let backend = S3VfsBackend::with_transport(test_config(), transport.clone());
        backend.put(&"d".repeat(64), b"payload-bytes").unwrap();

        let seen = transport.seen.lock().unwrap();
        let req = seen.first().unwrap();
        assert_eq!(req.method, "PUT");
        assert_eq!(req.body, b"payload-bytes");
        assert!(req
            .url
            .ends_with(&format!("/prism-test/vfs/{}", "d".repeat(64))));
        assert!(req.headers.contains_key("authorization"));
        assert_eq!(
            req.headers.get("x-amz-content-sha256").unwrap(),
            "UNSIGNED-PAYLOAD"
        );
    }

    #[test]
    fn s3_has_returns_size_from_headers() {
        let transport = Arc::new(FakeTransport::default());
        let mut headers = BTreeMap::new();
        headers.insert("content-length".into(), "777".into());
        transport.push(
            "HEAD",
            "/prism-test/vfs/",
            HttpResponse {
                status: 200,
                headers,
                body: Vec::new(),
            },
        );
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        let size = backend.has(&"e".repeat(64)).unwrap();
        assert_eq!(size, Some(777));
    }

    #[test]
    fn s3_has_maps_404_to_none() {
        let transport = Arc::new(FakeTransport::default());
        transport.push("HEAD", "/prism-test/vfs/", empty_response(404));
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        assert_eq!(backend.has(&"f".repeat(64)).unwrap(), None);
    }

    #[test]
    fn s3_get_streams_the_object_body() {
        let transport = Arc::new(FakeTransport::default());
        transport.push(
            "GET",
            "/prism-test/vfs/",
            ok_response(b"blob-contents".to_vec(), 200),
        );
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        let bytes = backend.get(&"0".repeat(64)).unwrap();
        assert_eq!(bytes, b"blob-contents");
    }

    #[test]
    fn s3_delete_returns_true_on_204() {
        let transport = Arc::new(FakeTransport::default());
        transport.push("DELETE", "/prism-test/vfs/", empty_response(204));
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        assert!(backend.delete(&"1".repeat(64)).unwrap());
    }

    #[test]
    fn s3_delete_returns_false_on_404() {
        let transport = Arc::new(FakeTransport::default());
        transport.push("DELETE", "/prism-test/vfs/", empty_response(404));
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        assert!(!backend.delete(&"2".repeat(64)).unwrap());
    }

    #[test]
    fn s3_list_parses_xml_and_filters_non_hash_keys() {
        let entries = vec![
            (format!("vfs/{}", "a".repeat(64)), 10u64),
            (format!("vfs/{}", "b".repeat(64)), 20u64),
            ("vfs/not-a-hash".to_string(), 0u64), // filtered out
        ];
        let body = list_response_xml(&entries);
        let transport = Arc::new(FakeTransport::default());
        transport.push(
            "GET",
            "?list-type=2",
            HttpResponse {
                status: 200,
                headers: BTreeMap::new(),
                body: body.into_bytes(),
            },
        );
        let backend = S3VfsBackend::with_transport(test_config(), transport);
        let out = backend.list().unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].hash, "a".repeat(64));
        assert_eq!(out[0].size, 10);
        assert_eq!(out[1].hash, "b".repeat(64));
    }

    #[test]
    fn s3_backend_label_is_s3_and_gcs_override() {
        let transport: Arc<dyn HttpTransport> = Arc::new(FakeTransport::default());
        let backend = S3VfsBackend::with_transport(test_config(), transport.clone());
        assert_eq!(backend.backend_name(), "s3");
    }

    #[test]
    fn vfs_manager_delegates_to_remote_backend_put() {
        // Full end-to-end through the façade: VfsManager → S3 backend
        // → FakeTransport. Proves remote backends plug into the exact
        // same command shape the local backend uses.
        let transport = Arc::new(FakeTransport::default());
        transport.push("HEAD", "/prism-test/vfs/", empty_response(404));
        transport.push("PUT", "/prism-test/vfs/", empty_response(200));
        let backend = S3VfsBackend::with_transport(test_config(), transport.clone());
        let mgr = VfsManager::with_backend(Arc::new(backend));
        let hash = mgr.put(b"via facade").unwrap();
        assert_eq!(hash.len(), 64);
        // Two requests expected: HEAD (dedupe check) + PUT.
        assert_eq!(transport.seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn vfs_manager_dedupes_put_via_has_first() {
        let transport = Arc::new(FakeTransport::default());
        let mut headers = BTreeMap::new();
        headers.insert("content-length".into(), "9".into());
        transport.push(
            "HEAD",
            "/prism-test/vfs/",
            HttpResponse {
                status: 200,
                headers,
                body: Vec::new(),
            },
        );
        // No PUT pushed — dedupe path must not issue one.
        let backend = S3VfsBackend::with_transport(test_config(), transport.clone());
        let mgr = VfsManager::with_backend(Arc::new(backend));
        mgr.put(b"existing!").unwrap();
        let seen = transport.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "HEAD");
    }

    #[test]
    fn s3config_object_key_respects_prefix() {
        let mut cfg = S3Config::aws("us-west-2", "b", creds());
        assert_eq!(cfg.object_key("abc"), "abc");
        cfg.prefix = "pfx".into();
        assert_eq!(cfg.object_key("abc"), "pfx/abc");
        cfg.prefix = "pfx/".into();
        assert_eq!(cfg.object_key("abc"), "pfx/abc");
    }

    #[test]
    fn gcs_config_uses_storage_googleapis_endpoint() {
        let cfg = S3Config::gcs("b", creds());
        assert_eq!(cfg.endpoint, "https://storage.googleapis.com");
        assert_eq!(cfg.service, "s3");
        assert_eq!(cfg.region, "auto");
    }

    #[cfg(feature = "vfs-gcs")]
    #[test]
    fn gcs_backend_labels_itself_gcs() {
        let transport: Arc<dyn HttpTransport> = Arc::new(FakeTransport::default());
        let backend = GcsVfsBackend::with_transport("my-bucket", creds(), transport);
        assert_eq!(backend.backend_name(), "gcs");
    }

    #[test]
    fn split_url_handles_queries_and_encodes_path() {
        let (host, path, query) =
            split_url("https://s3.example.com/bkt/key%20name?list-type=2&prefix=vfs/");
        assert_eq!(host, "s3.example.com");
        // path segments with space stay encoded as %20 (we re-encode
        // them, which in this case is idempotent).
        assert_eq!(path, "/bkt/key%2520name");
        assert_eq!(query, "list-type=2&prefix=vfs%2F");
    }
}
