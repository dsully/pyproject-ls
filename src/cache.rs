use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::Arc,
};

use jiff::Timestamp;
use moka::future::Cache as MokaCache;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::types::PackageResponse;

const CACHE_FILE: &str = "pypi_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPackage {
    pub data: PackageResponse,
    pub last_updated: Timestamp,
}

#[derive(Debug, Clone)]
pub struct Cache {
    #[allow(clippy::struct_field_names)]
    memory_cache: MokaCache<String, CachedPackage>,
    disk_cache_path: PathBuf,
    disk_cache_mutex: Arc<Mutex<()>>,
}

impl Cache {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
        let disk_cache_path = cache_dir.join(CACHE_FILE);
        let memory_cache = MokaCache::new(1000);

        Cache {
            memory_cache,
            disk_cache_path,
            disk_cache_mutex: Arc::new(Mutex::new(())),
        }
    }

    pub async fn get_package(&self, package_name: &str) -> Option<CachedPackage> {
        if let Some(cached) = self.memory_cache.get(package_name).await {
            return Some(cached);
        }

        let _guard = self.disk_cache_mutex.lock().await;
        if let Some(cached) = self.load_from_disk(package_name) {
            self.memory_cache.insert(package_name.to_string(), cached.clone()).await;
            return Some(cached);
        }

        None
    }

    pub async fn insert_package(&self, package_name: String, data: PackageResponse) {
        let cached = CachedPackage {
            data,
            last_updated: Timestamp::now(),
        };

        self.memory_cache.insert(package_name.clone(), cached.clone()).await;

        let _guard = self.disk_cache_mutex.lock().await;

        self.save_to_disk(package_name, cached).await;
    }

    fn load_from_disk(&self, package_name: &str) -> Option<CachedPackage> {
        //
        let Ok(file) = File::open(&self.disk_cache_path) else {
            return None;
        };

        let reader = BufReader::new(file);
        let cache_data: Result<std::collections::HashMap<String, CachedPackage>, _> = serde_json::from_reader(reader);

        match cache_data {
            Ok(cache_data) => cache_data.get(package_name).cloned(),
            Err(_) => None,
        }
    }

    async fn save_to_disk(&self, package_name: String, cached: CachedPackage) {
        let _guard = self.disk_cache_mutex.lock().await;

        let mut cache_data = match File::open(&self.disk_cache_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                serde_json::from_reader(reader).unwrap_or_else(|_| std::collections::HashMap::new())
            }
            Err(_) => std::collections::HashMap::new(),
        };

        cache_data.insert(package_name, cached);

        let file = File::create(&self.disk_cache_path).expect("Unable to create cache file");
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, &cache_data).expect("Unable to write cache file");
    }
}
