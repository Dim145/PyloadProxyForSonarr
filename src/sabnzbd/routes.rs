use crate::error::ProxyError;
use crate::pyload::{DownloadInfo, FileData, Package};
use crate::sabnzbd::models::*;
use crate::state::AppState;
use axum::{
    Json,
    extract::{FromRequest, Multipart, Query, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

const SAB_VERSION: &str = "4.3.0";

#[derive(Deserialize, Debug, Default)]
pub struct ApiParams {
    pub mode: Option<String>,
    pub apikey: Option<String>,
    pub name: Option<String>,
    pub value: Option<String>,
    pub cat: Option<String>,
    pub nzbname: Option<String>,
    pub start: Option<u32>,
    pub limit: Option<u32>,
}

pub async fn api_endpoint(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ApiParams>,
    request: Request,
) -> Result<Response, ProxyError> {
    if params.apikey.as_deref() != Some(state.config.api_key.as_str()) {
        return Err(ProxyError::Unauthorized);
    }
    let mode = params
        .mode
        .as_deref()
        .ok_or_else(|| ProxyError::BadRequest("missing mode".into()))?;
    tracing::debug!(mode, name = ?params.name, "sab api call");

    match mode {
        "version" => Ok(Json(VersionResponse { version: SAB_VERSION }).into_response()),
        "get_config" => Ok(Json(build_config(&state)).into_response()),
        "fullstatus" => Ok(Json(build_full_status(&state).await?).into_response()),
        "queue" => match params.name.as_deref() {
            Some("delete") => delete_value(&state, params.value.as_deref()).await,
            _ => build_queue(&state, &params).await,
        },
        "history" => match params.name.as_deref() {
            Some("delete") => delete_value(&state, params.value.as_deref()).await,
            _ => build_history(&state, &params).await,
        },
        "addurl" => add_url(&state, &params).await,
        "addfile" => {
            let multipart = Multipart::from_request(request, &())
                .await
                .map_err(|e| ProxyError::BadRequest(format!("multipart: {e}")))?;
            add_file(&state, &params, multipart).await
        }
        "pause" | "resume" | "shutdown" => Ok(Json(json!({"status": true})).into_response()),
        "warnings" => Ok(Json(json!({"warnings": []})).into_response()),
        "options" => Ok(Json(json!({})).into_response()),
        other => Err(ProxyError::BadRequest(format!("unsupported mode: {other}"))),
    }
}

fn build_config(state: &AppState) -> ConfigResponse {
    ConfigResponse {
        config: ConfigPayload {
            misc: ConfigMisc {
                complete_dir: state.config.download_dir.clone(),
                pre_check: false,
                history_retention: "0".into(),
            },
            categories: vec![
                Category {
                    name: "*".into(),
                    order: 0,
                    pp: "3".into(),
                    script: "None".into(),
                    dir: String::new(),
                    priority: -100,
                },
                Category {
                    name: state.config.default_category.clone(),
                    order: 1,
                    pp: "3".into(),
                    script: "None".into(),
                    dir: String::new(),
                    priority: 0,
                },
            ],
        },
    }
}

async fn build_full_status(state: &AppState) -> Result<FullStatusResponse, ProxyError> {
    let status = state.pyload.status().await?;
    Ok(FullStatusResponse {
        status: FullStatus {
            paused: status.pause,
            pause_int: "0".into(),
            remaining_quota: "0".into(),
            have_quota: false,
            speed: status.speed.to_string(),
            diskspace1: "0".into(),
            diskspace2: "0".into(),
            diskspacetotal1: "0".into(),
            diskspacetotal2: "0".into(),
        },
    })
}

fn nzo_id(pid: i64) -> String {
    format!("pyld_{pid}")
}

fn parse_nzo_id(value: &str) -> Option<i64> {
    value.strip_prefix("pyld_").and_then(|s| s.parse().ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageState {
    Downloading,
    Queued,
    Paused,
    Completed,
    Failed,
}

impl PackageState {
    fn as_sab(self) -> &'static str {
        match self {
            Self::Downloading => "Downloading",
            Self::Queued => "Queued",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }

    fn is_in_flight(self) -> bool {
        matches!(self, Self::Downloading | Self::Queued | Self::Paused)
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

fn classify_link(status: i64) -> LinkClass {
    match status {
        0 => LinkClass::Finished,
        7 | 10 | 12 | 13 => LinkClass::Active,
        2 | 3 | 5 => LinkClass::Waiting,
        1 | 6 | 8 | 9 => LinkClass::Failed,
        _ => LinkClass::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkClass {
    Finished,
    Active,
    Waiting,
    Failed,
    Other,
}

fn package_state(p: &Package, server_paused: bool) -> PackageState {
    let links = p.links.as_deref().unwrap_or(&[]);
    if !links.is_empty() {
        let total = links.len();
        let mut finished = 0usize;
        let mut active = 0usize;
        let mut waiting = 0usize;
        let mut failed = 0usize;
        for l in links {
            match classify_link(l.status) {
                LinkClass::Finished => finished += 1,
                LinkClass::Active => active += 1,
                LinkClass::Waiting => waiting += 1,
                LinkClass::Failed => failed += 1,
                LinkClass::Other => {}
            }
        }
        if finished == total {
            return PackageState::Completed;
        }
        if failed + finished == total && failed > 0 {
            return PackageState::Failed;
        }
        if server_paused {
            return PackageState::Paused;
        }
        if active > 0 {
            return PackageState::Downloading;
        }
        if waiting > 0 {
            return PackageState::Queued;
        }
        return PackageState::Queued;
    }

    let lt = p.linkstotal.unwrap_or(0);
    let ld = p.linksdone.unwrap_or(0);
    let st = p.sizetotal.unwrap_or(0);
    let sd = p.sizedone.unwrap_or(0);
    if (lt > 0 && ld >= lt) || (st > 0 && sd >= st) {
        return PackageState::Completed;
    }
    if server_paused {
        return PackageState::Paused;
    }
    if sd > 0 {
        PackageState::Downloading
    } else {
        PackageState::Queued
    }
}

async fn fetch_all_packages(state: &AppState) -> Vec<Package> {
    let mut all = state.pyload.queue().await.unwrap_or_default();
    if let Ok(coll) = state.pyload.collector().await {
        all.extend(coll);
    }
    all
}

fn package_size(p: &Package) -> i64 {
    if let Some(total) = p.sizetotal {
        if total > 0 {
            return total;
        }
    }
    p.links
        .as_deref()
        .map(|ls| ls.iter().map(|l: &FileData| l.size.max(0)).sum())
        .unwrap_or(0)
}

fn compute_progress(
    p: &Package,
    dls: &[&DownloadInfo],
    fallback_speed: i64,
    now: f64,
) -> (i64, i64, i64, i64) {
    let pkg_total = package_size(p).max(0);
    if dls.is_empty() {
        let done = p.sizedone.unwrap_or(0).max(0).min(pkg_total);
        return (pkg_total, done, fallback_speed, 0);
    }
    let live_total: i64 = dls.iter().map(|d| d.size.max(0)).sum();
    let total = pkg_total.max(live_total);
    let bleft: i64 = dls.iter().map(|d| d.bleft.max(0)).sum();
    let done = (total - bleft).max(0).min(total);
    let speed: i64 = dls.iter().map(|d| d.speed.max(0)).sum();
    let eta = dls
        .iter()
        .map(|d| {
            let wait = ((d.wait_until - now).max(0.0)) as i64;
            d.eta.max(0).max(wait)
        })
        .max()
        .unwrap_or(0);
    (total, done, speed, eta)
}

fn format_timeleft(secs: i64) -> String {
    let s = secs.max(0);
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn unix_now() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn format_size(bytes: i64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{:.2} MB", mb)
    }
}

async fn build_queue(state: &AppState, params: &ApiParams) -> Result<Response, ProxyError> {
    let packages = fetch_all_packages(state).await;
    let status = state.pyload.status().await.ok();
    let global_speed = status.as_ref().map(|s| s.speed).unwrap_or(0);
    let paused = status.as_ref().map(|s| s.pause).unwrap_or(false);
    let downloads = state.pyload.downloads().await.unwrap_or_default();
    let now = unix_now();

    let active: Vec<(&Package, PackageState)> = packages
        .iter()
        .map(|p| (p, package_state(p, paused)))
        .filter(|(_, s)| s.is_in_flight())
        .collect();

    let mut total_mb = 0.0_f64;
    let mut total_mbleft = 0.0_f64;

    let slots: Vec<QueueSlot> = active
        .iter()
        .map(|(p, s)| {
            let dls: Vec<&DownloadInfo> =
                downloads.iter().filter(|d| d.package_id == p.pid).collect();
            let (total, done, _, eta_secs) = compute_progress(p, &dls, global_speed, now);
            let left = (total - done).max(0);
            total_mb += total as f64 / (1024.0 * 1024.0);
            total_mbleft += left as f64 / (1024.0 * 1024.0);
            let percentage = if total > 0 { ((done * 100) / total) as u32 } else { 0 };
            let timeleft = if eta_secs > 0 && *s != PackageState::Paused {
                format_timeleft(eta_secs)
            } else {
                "0:00:00".into()
            };
            QueueSlot {
                nzo_id: nzo_id(p.pid),
                filename: p.name.clone(),
                status: s.as_sab().into(),
                cat: params
                    .cat
                    .clone()
                    .unwrap_or_else(|| state.config.default_category.clone()),
                mb: format!("{:.2}", total as f64 / (1024.0 * 1024.0)),
                mbleft: format!("{:.2}", left as f64 / (1024.0 * 1024.0)),
                size: format_size(total),
                sizeleft: format_size(left),
                percentage: percentage.to_string(),
                priority: "Normal".into(),
                script: "None".into(),
                timeleft,
            }
        })
        .collect();

    let speed = downloads.iter().map(|d| d.speed).sum::<i64>().max(global_speed);

    let start = params.start.unwrap_or(0);
    let limit = params.limit.unwrap_or(slots.len() as u32);

    Ok(Json(QueueResponse {
        queue: Queue {
            paused,
            slots,
            speed: speed.to_string(),
            speedlimit: "0".into(),
            mb: format!("{total_mb:.2}"),
            mbleft: format!("{total_mbleft:.2}"),
            noofslots: active.len(),
            noofslots_total: active.len(),
            start,
            limit,
        },
    })
    .into_response())
}

async fn build_history(state: &AppState, params: &ApiParams) -> Result<Response, ProxyError> {
    let packages = fetch_all_packages(state).await;
    let server_paused = state
        .pyload
        .status()
        .await
        .ok()
        .map(|s| s.pause)
        .unwrap_or(false);
    let download_dir = state.config.download_dir.trim_end_matches('/');
    let category = params
        .cat
        .clone()
        .unwrap_or_else(|| state.config.default_category.clone());

    let slots: Vec<HistorySlot> = packages
        .iter()
        .map(|p| (p, package_state(p, server_paused)))
        .filter(|(_, s)| s.is_terminal())
        .map(|(p, s)| {
            let folder = if p.folder.is_empty() { &p.name } else { &p.folder };
            let fail_message = if s == PackageState::Failed {
                p.links
                    .as_deref()
                    .and_then(|ls| ls.iter().find(|l| !l.error.is_empty()).map(|l| l.error.clone()))
                    .unwrap_or_else(|| "download failed".into())
            } else {
                String::new()
            };
            HistorySlot {
                nzo_id: nzo_id(p.pid),
                name: p.name.clone(),
                nzb_name: p.name.clone(),
                category: category.clone(),
                status: s.as_sab().into(),
                storage: format!("{download_dir}/{folder}"),
                bytes: package_size(p),
                download_time: 0,
                fail_message,
                script_line: String::new(),
            }
        })
        .collect();

    Ok(Json(HistoryResponse {
        history: History {
            noofslots: slots.len(),
            slots,
        },
    })
    .into_response())
}

async fn add_url(state: &AppState, params: &ApiParams) -> Result<Response, ProxyError> {
    let url = params
        .name
        .clone()
        .ok_or_else(|| ProxyError::BadRequest("missing name (url)".into()))?;
    let display_name = params
        .nzbname
        .clone()
        .unwrap_or_else(|| extract_name(&url));
    let pid = state
        .pyload
        .add_package(&display_name, &[url], state.config.pyload_dest)
        .await?;
    tracing::info!(pid, name = %display_name, "added pyload package via addurl");
    Ok(Json(AddUrlResponse {
        status: true,
        nzo_ids: vec![nzo_id(pid)],
    })
    .into_response())
}

async fn add_file(
    state: &AppState,
    params: &ApiParams,
    mut multipart: Multipart,
) -> Result<Response, ProxyError> {
    let mut filename: Option<String> = None;
    let mut content: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ProxyError::BadRequest(e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "name" || field_name == "nzbfile" {
            filename = field.file_name().map(|s| s.to_string());
            content = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ProxyError::BadRequest(e.to_string()))?
                    .to_vec(),
            );
            break;
        }
    }
    let content = content.ok_or_else(|| ProxyError::BadRequest("no file field".into()))?;
    let text = String::from_utf8_lossy(&content);
    let urls: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("http://") || l.starts_with("https://") || l.starts_with("ftp://"))
        .map(String::from)
        .collect();
    if urls.is_empty() {
        return Err(ProxyError::BadRequest(
            "no URLs found in uploaded file".into(),
        ));
    }
    let display_name = params
        .nzbname
        .clone()
        .or(filename)
        .unwrap_or_else(|| "sonarr-upload".into());
    let pid = state
        .pyload
        .add_package(&display_name, &urls, state.config.pyload_dest)
        .await?;
    tracing::info!(
        pid,
        name = %display_name,
        links = urls.len(),
        "added pyload package via addfile"
    );
    Ok(Json(AddUrlResponse {
        status: true,
        nzo_ids: vec![nzo_id(pid)],
    })
    .into_response())
}

async fn delete_value(state: &AppState, value: Option<&str>) -> Result<Response, ProxyError> {
    let value = value.ok_or_else(|| ProxyError::BadRequest("missing value".into()))?;
    if value == "all" {
        let mut pids: Vec<i64> = state.pyload.queue().await?.into_iter().map(|p| p.pid).collect();
        if let Ok(coll) = state.pyload.collector().await {
            pids.extend(coll.into_iter().map(|p| p.pid));
        }
        if !pids.is_empty() {
            state.pyload.delete_packages(&pids).await?;
        }
        return Ok(Json(json!({"status": true, "nzo_ids": []})).into_response());
    }
    let pids: Vec<i64> = value.split(',').filter_map(parse_nzo_id).collect();
    if pids.is_empty() {
        return Err(ProxyError::BadRequest(format!(
            "could not parse value: {value}"
        )));
    }
    state.pyload.delete_packages(&pids).await?;
    let ids: Vec<&str> = value.split(',').collect();
    Ok(Json(json!({"status": true, "nzo_ids": ids})).into_response())
}

fn extract_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_string()
}

pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    match state.pyload.version().await {
        Ok(v) => (
            StatusCode::OK,
            Json(json!({"status": "ok", "pyload_version": v})),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "healthcheck: pyload unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "unhealthy"})),
            )
                .into_response()
        }
    }
}
