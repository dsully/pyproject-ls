use std::time::Duration;

use moka::future::Cache;
use pep440_rs::Version;
use reqwest::{Client, Error, Response, Url};
use tracing::info;

use crate::types::{PackageFile, PackageResponse};

const PYPI_BASE_URL: &str = "https://pypi.org/pypi";

pub async fn fetch_package_info(package_name: &str) -> Result<PackageResponse, reqwest::Error> {
    let url = format!("{PYPI_BASE_URL}/{package_name}/json");

    info!("Fetching package info from: {url}");

    let client = Client::new();
    let response = client.get(url).send().await?;

    response.json().await
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct CacheEntry {
    package_info: PackageResponse,
    etag: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PypiClient {
    base_url: Url,
    cache: Cache<String, CacheEntry>,
    client: Client,
}

#[allow(dead_code)]
impl PypiClient {
    pub fn new() -> Self {
        let cache = Cache::builder()
            .max_capacity(100)
            .time_to_live(Duration::from_secs(3600)) // 1 hour
            .time_to_idle(Duration::from_secs(1800)) // 30 minutes
            .build();

        Self {
            base_url: Url::parse(PYPI_BASE_URL).expect("Unable to parse the URL"),
            cache,
            client: Client::new(),
        }
    }

    async fn make_request(&self, url: Url, etag: Option<&str>) -> Result<Response, Error> {
        //
        let mut request = self
            .client
            .get(url.clone())
            .header("Accept", "application/vnd.pypi.simple.v1+json");

        // Add If-None-Match header if we have an etag
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }

        info!("GET: {url}");

        request.send().await
    }

    pub async fn package_info(&self, package_name: &str) -> anyhow::Result<PackageResponse> {
        //
        // The trailing slash is important.
        let url = self.base_url.join(&format!("{package_name}/"))?;

        // Check cache first
        if let Some(cached) = self.cache.get(package_name).await {
            let response = self.make_request(url.clone(), cached.etag.as_deref()).await?;

            // If we get a 304 Not Modified, return cached data
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(cached.package_info);
            }

            // If we get a successful response, update cache
            if response.status().is_success() {
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|h| h.to_str().ok())
                    .map(String::from);

                let package_info = response.json::<PackageResponse>().await?;

                let cache_entry = CacheEntry {
                    package_info: package_info.clone(),
                    etag,
                };

                self.cache.insert(package_name.to_string(), cache_entry).await;
                return Ok(package_info);
            }
        }

        // If not in cache or cache validation failed, make a fresh request
        let response = self.make_request(url, None).await?;

        // Store headers we might want to use
        let _etag = response.headers().get("etag").and_then(|h| h.to_str().ok());

        let _last_serial = response
            .headers()
            .get("x-pypi-last-serial")
            .and_then(|h| h.to_str().ok());

        if !response.status().is_success() {
            // return Err(response.error_for_status()?);
            // return Err(Error::status(response.status()));
        }

        let etag = response
            .headers()
            .get("etag")
            .and_then(|h| h.to_str().ok())
            .map(String::from);

        let package_info = response.json::<PackageResponse>().await?;

        let cache_entry = CacheEntry {
            package_info: package_info.clone(),
            etag,
        };

        self.cache.insert(package_name.to_string(), cache_entry).await;

        Ok(package_info)
    }

    // Helper method to get the latest version of a package
    pub async fn latest_version(&self, package: &str) -> anyhow::Result<Option<PackageFile>> {
        let package_info = self.package_info(package).await?;

        // Find the most recent non-yanked release
        let latest_file = package_info
            .files
            .into_iter()
            .filter(|file| !file.yanked)
            .max_by_key(|file| file.upload_time);

        Ok(latest_file)
    }

    // Helper method to get all versions of a package
    pub async fn versions(&self, package: &str) -> anyhow::Result<Vec<Version>> {
        let info = self.package_info(package).await?;

        Ok(info.versions)
    }
}
