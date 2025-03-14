use std::str::FromStr;
use std::sync::Arc;

use itertools::Itertools;
// use pyproject_toml::{DependencyGroupSpecifier, DependencyGroups, PyProjectToml};
use toml_edit::{DocumentMut, Value};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionResponse, Position, Range};
use tracing::info;

use crate::{cache::Cache, pypi};

#[derive(Debug, Clone)]
struct DependencyWithSpan {
    name: String,
    range: Range,
}

pub async fn get_completions(text: &str, position: Position, cache: Arc<Cache>) -> Option<CompletionResponse> {
    //
    let Ok(document) = DocumentMut::from_str(text) else {
        return None;
    };

    let dependencies_with_spans = extract_dependencies_with_spans(&document, text);

    let (dependency_name, _dependency_range) = find_dependency_at_position(&dependencies_with_spans, position)?;

    let completions = if dependency_name.is_empty() {
        get_package_name_completions(cache).await
    } else {
        get_package_version_completions(dependency_name, cache).await
    };

    let items = completions
        .into_iter()
        .map(|item| CompletionItem {
            label: item.label,
            kind: item.kind,
            insert_text: item.insert_text,
            // range: Some(dependency_range),
            ..Default::default()
        })
        .collect();

    Some(CompletionResponse::Array(items))
}

fn extract_dependencies_with_spans(document: &DocumentMut, text: &str) -> Vec<DependencyWithSpan> {
    let mut dependencies_with_spans = Vec::new();

    if let Some(project) = document.get("project").and_then(|p| p.as_table()) {
        if let Some(deps) = project.get("dependencies").and_then(|d| d.as_array()) {
            //
            for dep in deps {
                if let Some(dep_str) = dep.as_str() {
                    //
                    if let Some(range) = get_range_for_value(dep, document, text) {
                        let parts: Vec<&str> = dep_str.split(&['<', '>', '=', '!']).collect();
                        let name = parts.first().unwrap_or(&"").trim().to_string();
                        dependencies_with_spans.push(DependencyWithSpan { name, range });
                    }
                }
            }
        }

        if let Some(optional_deps) = project.get("optional-dependencies").and_then(|d| d.as_table()) {
            //
            for (_group, deps) in optional_deps {
                //
                if let Some(deps_array) = deps.as_array() {
                    //
                    for dep in deps_array {
                        if let Some(dep_str) = dep.as_str() {
                            if let Some(range) = get_range_for_value(dep, document, text) {
                                let parts: Vec<&str> = dep_str.split(&['<', '>', '=', '!']).collect();
                                let name = parts.first().unwrap_or(&"").trim().to_string();
                                dependencies_with_spans.push(DependencyWithSpan { name, range });
                            }
                        }
                    }
                }
            }
        }
    }

    dependencies_with_spans
}

fn find_dependency_at_position(
    dependencies_with_spans: &Vec<DependencyWithSpan>,
    position: Position,
) -> Option<(String, Range)> {
    for dep_with_span in dependencies_with_spans {
        if is_position_in_range(position, dep_with_span.range) {
            return Some((dep_with_span.name.clone(), dep_with_span.range));
        }
    }
    None
}

fn get_range_for_value(value: &Value, _document: &DocumentMut, text: &str) -> Option<Range> {
    if let Some(span) = value.span() {
        let start_position = calculate_position(text, span.start);
        let end_position = calculate_position(text, span.end);

        Some(Range {
            start: start_position,
            end: end_position,
        })
    } else {
        None
    }
}

fn calculate_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;
    for (i, c) in text.chars().enumerate() {
        if i == offset {
            return Position { line, character };
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

fn is_position_in_range(position: Position, range: Range) -> bool {
    position.line >= range.start.line
        && position.line <= range.end.line
        && position.character >= range.start.character
        && position.character <= range.end.character
}

#[allow(unused_variables)]
async fn get_package_name_completions(_cache: Arc<Cache>) -> Vec<CompletionItem> {
    let url = "https://hugovk.github.io/top-pypi-packages/top-pypi-packages-30-days.min.json";
    let client = reqwest::Client::new();

    let response = match client.get(url).send().await {
        Ok(res) => res,
        Err(e) => {
            info!("Error fetching top packages: {e}");
            return vec![];
        }
    };

    let json: serde_json::Value = match response.json().await {
        Ok(json) => json,
        Err(e) => {
            info!("Error parsing top packages JSON: {e}");
            return vec![];
        }
    };

    let packages = json["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("project").and_then(|p| p.as_str()))
                .map(|name| CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::MODULE),
                    insert_text: Some(format!("\"{name}\"")),
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default();

    packages
}

async fn get_package_version_completions(package_name: String, cache: Arc<Cache>) -> Vec<CompletionItem> {
    let cached_package = cache.get_package(&package_name).await;

    let package_data = if let Some(cached) = cached_package {
        info!("Using cached package data for: {}", package_name);
        cached.data
    } else {
        info!("Fetching package data for: {}", package_name);
        match pypi::fetch_package_info(&package_name).await {
            Ok(data) => {
                cache.insert_package(package_name.clone(), data.clone()).await;
                data
            }
            Err(e) => {
                info!("Error fetching package info: {e}");
                return vec![];
            }
        }
    };

    let mut versions = package_data.versions.iter().collect_vec();

    versions.sort();
    versions.reverse();

    versions
        .into_iter()
        .map(|version| CompletionItem {
            label: version.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            insert_text: Some(format!("\"{version}\"")),
            ..Default::default()
        })
        .collect()
}

// use std::str::FromStr;
// use std::sync::Arc;
//
// use itertools::Itertools;
// use pep508_rs::Requirement;
// use pyproject_toml::{DependencyGroupSpecifier, DependencyGroups, PyProjectToml};
// use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionResponse, Position, Range};
// use tracing::info;
//
// use crate::{cache::Cache, pypi};
//
// pub async fn get_completions(text: &str, position: Position, cache: Arc<Cache>) -> Option<CompletionResponse> {
//     let document = match PyProjectToml::new(text) {
//         Ok(doc) => doc,
//         Err(_) => return None,
//     };
//
//     let (dependency_name, dependency_range) = find_dependency_at_position(&document, position)?;
//
//     let completions = if dependency_name.is_empty() {
//         get_package_name_completions(cache).await
//     } else {
//         get_package_version_completions(dependency_name, cache).await
//     };
//
//     let items = completions
//         .into_iter()
//         .map(|item| CompletionItem {
//             label: item.label,
//             kind: item.kind,
//             insert_text: item.insert_text,
//             // range: Some(dependency_range),
//             ..Default::default()
//         })
//         .collect();
//
//     Some(CompletionResponse::Array(items))
// }
//
// fn find_dependency_at_position(document: &PyProjectToml, position: Position) -> Option<(String, Range)> {
//     if let Some(project) = &document.project {
//         if let Some(dependencies) = &project.dependencies {
//             for dep in dependencies {
//                 let dep_range = get_range_for_dependency(dep, document)?;
//                 if is_position_in_range(position, dep_range) {
//                     let name = dep.name.to_string();
//                     return Some((name, dep_range));
//                 }
//             }
//         }
//
//         if let Some(optional_dependencies) = &project.optional_dependencies {
//             for (_group, deps) in optional_dependencies {
//                 for dep in deps {
//                     let dep_range = get_range_for_dependency(dep, document)?;
//
//                     if is_position_in_range(position, dep_range) {
//                         let name = dep.name.to_string();
//                         return Some((name, dep_range));
//                     }
//                 }
//             }
//         }
//     }
//
//     None
// }
//
// fn get_range_for_dependency(dep: &Requirement, document: &PyProjectToml) -> Option<Range> {
//     let start = document.position_for(dep.span().start())?;
//     let end = document.position_for(dep.span().end())?;
//     Some(Range { start, end })
// }
//
// fn is_position_in_range(position: Position, range: Range) -> bool {
//     position.line >= range.start.line
//         && position.line <= range.end.line
//         && position.character >= range.start.character
//         && position.character <= range.end.character
// }
//
// async fn get_package_name_completions(cache: Arc<Cache>) -> Vec<CompletionItem> {
//     let url = "https://hugovk.github.io/top-pypi-packages/top-pypi-packages-30-days.min.json";
//     let client = reqwest::Client::new();
//     let response = match client.get(url).send().await {
//         Ok(res) => res,
//         Err(e) => {
//             info!("Error fetching top packages: {}", e);
//             return vec![];
//         }
//     };
//
//     let json: serde_json::Value = match response.json().await {
//         Ok(json) => json,
//         Err(e) => {
//             info!("Error parsing top packages json: {}", e);
//             return vec![];
//         }
//     };
//
//     let packages = json["rows"]
//         .as_array()
//         .map(|rows| {
//             rows.iter()
//                 .filter_map(|row| row.get("project").and_then(|p| p.as_str()))
//                 .map(|name| CompletionItem {
//                     label: name.to_string(),
//                     kind: Some(CompletionItemKind::MODULE),
//                     insert_text: Some(format!("\"{}\"", name)),
//                     ..Default::default()
//                 })
//                 .collect()
//         })
//         .unwrap_or_default();
//
//     packages
// }
//
// async fn get_package_version_completions(package_name: String, cache: Arc<Cache>) -> Vec<CompletionItem> {
//     let cached_package = cache.get_package(&package_name).await;
//
//     let package_data = match cached_package {
//         Some(cached) => {
//             info!("Using cached package data for: {}", package_name);
//             cached.data
//         }
//         None => {
//             info!("Fetching package data for: {}", package_name);
//             match pypi::fetch_package_info(&package_name).await {
//                 Ok(data) => {
//                     cache.insert_package(package_name.clone(), data.clone()).await;
//                     data
//                 }
//                 Err(e) => {
//                     info!("Error fetching package info: {}", e);
//                     return vec![];
//                 }
//             }
//         }
//     };
//
//     let mut versions = package_data.versions.iter().collect_vec();
//
//     versions.sort();
//     versions.reverse();
//
//     versions
//         .into_iter()
//         .map(|version| CompletionItem {
//             label: version.to_string(),
//             kind: Some(CompletionItemKind::VALUE),
//             insert_text: Some(format!("\"{}\"", version)),
//             ..Default::default()
//         })
//         .collect()
// }
