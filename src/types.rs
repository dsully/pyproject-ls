use jiff::civil::DateTime;
use pep440_rs::Version;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageResponse {
    pub files: Vec<PackageFile>,
    pub meta: PackageMeta,
    pub name: String,
    pub versions: Vec<Version>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageMeta {
    #[serde(rename = "_last-serial")]
    pub last_serial: u64,
    #[serde(rename = "api-version")]
    pub api_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageFile {
    #[serde(rename = "core-metadata")]
    pub core_metadata: Option<CoreMetadata>,
    #[serde(rename = "data-dist-info-metadata")]
    pub data_dist_info_metadata: Option<DistInfoMetadata>,
    pub filename: String,
    pub hashes: FileHashes,
    #[serde(rename = "requires-python")]
    pub requires_python: Option<String>,
    pub size: u64,
    #[serde(rename = "upload-time")]
    pub upload_time: DateTime,
    pub url: String,
    pub yanked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreMetadata {
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistInfoMetadata {
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileHashes {
    pub sha256: String,
}
