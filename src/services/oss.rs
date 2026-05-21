use std::{collections::HashMap, path::Path, pin::Pin, str::from_utf8, sync::Arc, time::Duration};

use anyhow::anyhow;
use async_stream::stream;
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use hmac::{Hmac, Mac};
use quick_xml::events::{BytesStart, Event};
use regex::Regex;
use reqwest::{header::HeaderMap, Body, Method, Response, Url};
use rocket::{data::ToByteUnit, Data};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::{sync::Semaphore, task::JoinSet, time::sleep};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    entities::{
        config::{OSSConfig, ServiceConfig},
        datetime::DateTime,
        oss::ObjectMeta,
    },
    services::Inject,
};

#[derive(Clone)]
pub struct OSS {
    config: Arc<OSSConfig>,
    region: Arc<String>,
}

impl Inject for OSS {
    fn new(config: &ServiceConfig) -> Self {
        let config = config.oss.clone();
        let pattern = Regex::new(r"oss-(.*?)(-internal)?\.aliyuncs\.com").unwrap();
        let region = pattern
            .captures(&config.endpoint)
            .and_then(|caps| caps.get(1))
            .expect(&format!("Invalid endpoint '{}'", config.endpoint))
            .as_str()
            .to_owned();
        Self {
            config: Arc::new(config),
            region: Arc::new(region),
        }
    }
}

static GET_OBJECT_RANGE_SIZE: usize = 16 * 1024 * 1024; // 8MB
static PUT_OBJECT_MAX_SIZE: usize = 512 * 1024 * 1024; // 512MB
static MULTIPART_UPLOAD_THRESHOLD: usize = 16 * 1024 * 1024; // 16MB
static MULTIPART_UPLOAD_PART_SIZE: usize = 4 * 1024 * 1024; // 4MB
static MULTIPART_UPLOAD_WORKERS_NUM: usize = 3;

pub type Stream<T> = Pin<Box<dyn futures::Stream<Item = T> + Send>>;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MultipartUploadResult {
    part_number: u16,
    e_tag: String,
}

impl OSS {
    async fn head_object(&self, key: &str) -> anyhow::Result<ObjectMeta> {
        let response = self
            .request(
                key,
                Method::GET,
                HashMap::new(),
                HeaderMap::new(),
                Body::default(),
            )
            .await?;
        let headers = response.headers();
        let content_type = headers
            .get("Content-Type")
            .ok_or_else(|| anyhow!("Missing response header 'Content-Type'"))?
            .to_str()?
            .to_owned();
        let content_length = headers
            .get("Content-Length")
            .ok_or_else(|| anyhow!("Missing response header 'Content-Length'"))?
            .to_str()?
            .parse()?;
        Ok(ObjectMeta {
            content_type,
            content_length,
        })
    }

    pub async fn get_object<T: AsRef<str>>(
        &self,
        name: T,
    ) -> anyhow::Result<(Stream<Bytes>, ObjectMeta)> {
        let key = self.build_key(name)?;
        let meta = self.head_object(&key).await?;
        let content_length = meta.content_length;
        let self_cloned = self.clone();
        let stream = stream! {
            'outer: for start in (0..content_length).step_by(GET_OBJECT_RANGE_SIZE) {
                let end = (start + GET_OBJECT_RANGE_SIZE as u64 - 1).min(content_length - 1);
                for retry in 0..=3 {
                    if let Ok(mut stream) = self_cloned.get_object_range(&key, (start, end)).await {
                        loop {
                            match stream.next().await {
                                Some(Ok(chunk)) => yield chunk,
                                Some(Err(_)) => break,
                                None => continue 'outer,
                            }
                        }
                    }
                    if retry < 3 {
                        sleep(Duration::from_secs(retry + 1)).await;
                    } else {
                        break 'outer;
                    }
                }
            }
        };
        Ok((Box::pin(stream), meta))
    }

    async fn get_object_range(
        &self,
        key: &str,
        range: (u64, u64),
    ) -> anyhow::Result<Stream<Result<Bytes, reqwest::Error>>> {
        let mut headers = HeaderMap::new();
        let (start, end) = range;
        headers.insert("Range", format!("bytes={}-{}", start, end).parse()?);
        let response = self
            .request(key, Method::GET, HashMap::new(), headers, Body::default())
            .await?;
        Ok(Box::pin(response.bytes_stream()))
    }

    pub async fn put_object(&self, data: Data<'_>, meta: ObjectMeta) -> anyhow::Result<String> {
        let name = format!("{}.{}", Uuid::new_v4().to_string(), meta.extension()?);
        let key = self.build_key(&name)?;
        let ObjectMeta { content_type, .. } = meta;
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", content_type.parse()?);
        let content_disposition =
            format!("attachment; filename=\"{}\"", urlencoding::encode(&name));
        headers.insert("Content-Disposition", content_disposition.parse()?);
        if meta.content_length <= MULTIPART_UPLOAD_THRESHOLD as u64 {
            self.put_object_by_key(&key, data, headers).await?;
        } else {
            self.multipart_upload(&key, data, headers).await?;
        }
        Ok(name)
    }

    async fn put_object_by_key(
        &self,
        key: &str,
        data: Data<'_>,
        headers: HeaderMap,
    ) -> anyhow::Result<()> {
        let reader = data.open(MULTIPART_UPLOAD_THRESHOLD.bytes());
        let bytes = reader.into_bytes().await?.into_inner();
        self.request(key, Method::PUT, HashMap::new(), headers, bytes)
            .await?;
        Ok(())
    }

    async fn multipart_upload(
        &self,
        key: &str,
        data: Data<'_>,
        headers: HeaderMap,
    ) -> anyhow::Result<()> {
        let upload_id = self.initiate_multipart_upload(&key, headers).await?;
        let mut set = JoinSet::new();
        let semaphore = Arc::new(Semaphore::new(MULTIPART_UPLOAD_WORKERS_NUM));
        let reader = data.open(PUT_OBJECT_MAX_SIZE.bytes());
        let mut stream = ReaderStream::new(reader);
        let mut buffer = BytesMut::new();
        let mut part_number = 1;
        while let Some(chunk) = stream.next().await {
            buffer.extend(chunk?);
            while buffer.len() >= MULTIPART_UPLOAD_PART_SIZE {
                let part = buffer.split_to(MULTIPART_UPLOAD_PART_SIZE).freeze();
                let self_cloned = self.clone();
                let key_owned = key.to_owned();
                let upload_id_clone = upload_id.clone();
                let part_number_copied = part_number;
                let semaphore_cloned = semaphore.clone();
                set.spawn(async move {
                    let permit = semaphore_cloned.acquire().await?;
                    let result = self_cloned
                        .upload_part(&key_owned, part, &upload_id_clone, part_number_copied)
                        .await?;
                    drop(permit);
                    anyhow::Ok(result)
                });
                part_number += 1;
            }
        }
        if !buffer.is_empty() {
            let part = buffer.freeze();
            let self_cloned = self.clone();
            let key_owned = key.to_owned();
            let upload_id_clone = upload_id.clone();
            let part_number_copied = part_number;
            let semaphore_cloned = semaphore.clone();
            set.spawn(async move {
                let permit = semaphore_cloned.acquire().await?;
                let result = self_cloned
                    .upload_part(&key_owned, part, &upload_id_clone, part_number_copied)
                    .await?;
                drop(permit);
                anyhow::Ok(result)
            });
        }
        let mut parts = Vec::new();
        while let Some(result) = set.join_next().await {
            parts.push(result??);
        }
        parts.sort_by_key(|part| part.part_number);
        self.complete_multipart_upload(&key, &upload_id, parts)
            .await?;
        Ok(())
    }

    async fn initiate_multipart_upload(
        &self,
        key: &str,
        headers: HeaderMap,
    ) -> anyhow::Result<String> {
        let mut query = HashMap::new();
        query.insert("uploads".to_owned(), "".to_owned());
        let response = self
            .request(key, Method::POST, query, headers, Body::default())
            .await?;
        let xml = response.text().await?;
        let mut reader = quick_xml::Reader::from_str(&xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event()? {
                Event::Start(e) => {
                    if e.name().as_ref() == b"UploadId" {
                        break;
                    }
                }
                Event::Eof => return Err(anyhow!("Missing element 'UploadId'")),
                _ => (),
            }
        }
        if let Event::Text(text) = reader.read_event()? {
            Ok(text.decode()?.into_owned())
        } else {
            Err(anyhow!("Failed to parse 'UploadId'"))
        }
    }

    async fn upload_part(
        &self,
        key: &str,
        data: Bytes,
        upload_id: &str,
        part_number: u16,
    ) -> anyhow::Result<MultipartUploadResult> {
        let mut query = HashMap::new();
        query.insert("uploadId".to_owned(), upload_id.to_owned());
        query.insert("partNumber".to_owned(), part_number.to_string());
        for retry in 0..=3 {
            match self
                .request(
                    key,
                    Method::PUT,
                    query.clone(),
                    HeaderMap::new(),
                    data.clone(),
                )
                .await
            {
                Ok(response) => {
                    let e_tag = response
                        .headers()
                        .get("ETag")
                        .ok_or_else(|| anyhow!("Missing response header 'ETag'"))?
                        .to_str()?
                        .to_owned();
                    return Ok(MultipartUploadResult { part_number, e_tag });
                }
                Err(err) => {
                    if retry < 3 {
                        sleep(Duration::from_secs(retry + 1)).await;
                    } else {
                        return Err(anyhow!(
                            "Failed to upload part (part_number={}) after {} retries: {:#}",
                            part_number,
                            retry,
                            err
                        ));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<MultipartUploadResult>,
    ) -> anyhow::Result<()> {
        let mut query = HashMap::new();
        query.insert("uploadId".to_owned(), upload_id.to_owned());
        let content = {
            let mut buffer = Vec::new();
            let mut writer = quick_xml::Writer::new_with_indent(&mut buffer, b' ', 4);
            let start = BytesStart::new("CompleteMultipartUpload");
            writer.write_event(Event::Start(start.clone()))?;
            for part in parts {
                writer.write_serializable("Part", &part)?;
            }
            writer.write_event(Event::End(start.to_end()))?;
            from_utf8(&buffer)?.to_owned()
        };
        self.request(key, Method::POST, query, HeaderMap::new(), content)
            .await?;
        Ok(())
    }

    fn build_key<T: AsRef<str>>(&self, name: T) -> anyhow::Result<String> {
        if Path::new(name.as_ref())
            .parent()
            .is_some_and(|p| p != Path::new(""))
        {
            return Err(anyhow!("Invalid file name: {}", name.as_ref()));
        }
        let path = Path::new("/").join(&self.config.prefix).join(name.as_ref());
        let key = path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid file path: {}", path.display()))?
            .to_owned();
        Ok(key)
    }

    async fn request<T: Into<Body>>(
        &self,
        key: &str,
        method: Method,
        query: HashMap<String, String>,
        mut headers: HeaderMap,
        data: T,
    ) -> anyhow::Result<Response> {
        let host = format!("{}.{}", self.config.bucket, self.config.endpoint);
        let url = Url::parse(&format!(
            "http://{}{}",
            host,
            urlencoding::encode(key).replace("%2F", "/")
        ))?;
        let mut additional_headers = headers
            .keys()
            .map(|key| key.as_str().to_lowercase())
            .collect::<Vec<_>>();
        additional_headers.sort_by(|val1, val2| val1.cmp(val2));
        headers.insert("Host", host.parse()?);
        let date = DateTime::utc();
        headers.insert("Date", date.format("%a, %d %b %Y %H:%M:%S GMT").parse()?);
        headers.insert("x-oss-date", date.format("%Y%m%dT%H%M%SZ").parse()?);
        headers.insert("x-oss-content-sha256", "UNSIGNED-PAYLOAD".parse()?);
        let auth = self.authorize_v4(key, &method, &query, &headers, additional_headers)?;
        headers.insert("Authorization", auth.parse()?);
        let response = reqwest::Client::new()
            .request(method, url)
            .query(&query)
            .headers(headers)
            .body(data)
            .send()
            .await?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(anyhow!(
                "Request failed ({}): {}",
                response.status(),
                response.text().await?
            ))
        }
    }

    fn authorize_v4(
        &self,
        key: &str,
        method: &Method,
        query: &HashMap<String, String>,
        headers: &HeaderMap,
        additional_headers: Vec<String>,
    ) -> anyhow::Result<String> {
        let datetime_iso8601 = headers
            .get("x-oss-date")
            .ok_or_else(|| anyhow!("Missing request header 'x-oss-date'"))?
            .to_str()?
            .to_owned();
        let sign_date = datetime_iso8601.split_once("T").unwrap().0.to_owned();
        let mut auth = format!(
            "OSS4-HMAC-SHA256 Credential={}/{}/{}/oss/aliyun_v4_request",
            self.config.access_key_id, sign_date, self.region,
        );
        if !additional_headers.is_empty() {
            auth += &format!(", AdditionalHeaders={}", additional_headers.join(";"));
        }
        let signature = self.sign_v4(
            key,
            method,
            query,
            headers,
            additional_headers,
            &datetime_iso8601,
            &sign_date,
        )?;
        auth += &format!(", Signature={}", signature);
        Ok(auth)
    }

    fn sign_v4(
        &self,
        key: &str,
        method: &Method,
        query: &HashMap<String, String>,
        headers: &HeaderMap,
        additional_headers: Vec<String>,
        datetime_iso8601: &str,
        sign_date: &str,
    ) -> anyhow::Result<String> {
        let scope = format!("{}/{}/oss/aliyun_v4_request", sign_date, self.region);
        let uri =
            urlencoding::encode(&format!("/{}{}", self.config.bucket, key)).replace("%2F", "/");
        let canonical_query = if query.is_empty() {
            String::new()
        } else {
            let mut sorted = query.iter().collect::<Vec<_>>();
            sorted.sort_by(|(key1, _), (key2, _)| key1.cmp(key2));
            sorted
                .into_iter()
                .map(|(key, value)| {
                    if value.is_empty() {
                        urlencoding::encode(key).to_string()
                    } else {
                        format!(
                            "{}={}",
                            urlencoding::encode(key),
                            urlencoding::encode(value)
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("&")
        };
        let content_sha256 = headers
            .get("x-oss-content-sha256")
            .ok_or_else(|| anyhow!("Missing request header 'x-oss-content-sha256'"))?
            .to_str()?;
        let canonical_headers = {
            let mut signed = HashMap::new();
            signed.insert("x-oss-content-sha256".to_owned(), content_sha256);
            let conditional_headers = vec!["content-type", "content-md5"];
            for (key, value) in headers {
                let key = key.as_str().to_lowercase();
                if conditional_headers.contains(&key.as_str())
                    || key.starts_with("x-oss-")
                    || additional_headers.contains(&key)
                {
                    signed.insert(key, value.to_str()?.trim());
                }
            }
            let mut sorted = signed.into_iter().collect::<Vec<_>>();
            sorted.sort_by(|(key1, _), (key2, _)| key1.cmp(key2));
            sorted
                .into_iter()
                .map(|(key, value)| format!("{}:{}", key, value))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n\n{}\n{}",
            method,
            uri,
            canonical_query,
            canonical_headers,
            additional_headers.join(";"),
            content_sha256,
        );
        let string_to_sign = format!(
            "OSS4-HMAC-SHA256\n{}\n{}\n{:x}",
            datetime_iso8601,
            scope,
            Sha256::digest(canonical_request)
        );
        let date_key = self.hmac_sha256(
            format!("aliyun_v4{}", self.config.access_key_secret),
            sign_date,
        )?;
        let date_region_key = self.hmac_sha256(date_key, self.region.as_ref())?;
        let date_region_service_key = self.hmac_sha256(date_region_key, "oss")?;
        let signing_key = self.hmac_sha256(date_region_service_key, "aliyun_v4_request")?;
        let signature = self.hmac_sha256(signing_key, string_to_sign)?;
        Ok(format!("{:x}", signature))
    }

    fn hmac_sha256<K: AsRef<[u8]>, T: AsRef<[u8]>>(
        &self,
        key: K,
        data: T,
    ) -> anyhow::Result<hmac::digest::Output<Sha256>> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_ref())?;
        mac.update(data.as_ref());
        Ok(mac.finalize().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_oss() -> OSS {
        OSS {
            config: Arc::new(OSSConfig {
                bucket: "oss-rocket-agentx".to_owned(),
                endpoint: "oss-cn-hangzhou.aliyuncs.com".to_owned(),
                access_key_id: "".to_owned(),
                access_key_secret: "".to_owned(),
                prefix: "/".to_owned(),
            }),
            region: Arc::new("cn-hangzhou".to_owned()),
        }
    }

    #[tokio::test]
    async fn test_head_object() {
        let oss = build_oss();
        let key = oss.build_key("Rust 程序设计语言 简体中文版.pdf").unwrap();
        let meta = oss.head_object(&key).await.unwrap();
        println!("{:?}", meta);
    }

    #[tokio::test]
    async fn test_get_object() {
        let oss = build_oss();
        let (mut stream, meta) = oss
            .get_object("Rust 程序设计语言 简体中文版.pdf")
            .await
            .unwrap();
        println!("{:?}", meta);
        while let Some(chunk) = stream.next().await {
            println!("{}", chunk.len());
        }
    }
}
