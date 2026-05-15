use serde::Serialize;

#[derive(Serialize)]
pub struct VersionResponse<'a> {
    pub version: &'a str,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub config: ConfigPayload,
}

#[derive(Serialize)]
pub struct ConfigPayload {
    pub misc: ConfigMisc,
    pub categories: Vec<Category>,
}

#[derive(Serialize)]
pub struct ConfigMisc {
    pub complete_dir: String,
    pub pre_check: bool,
    pub history_retention: String,
}

#[derive(Serialize)]
pub struct Category {
    pub name: String,
    pub order: u32,
    pub pp: String,
    pub script: String,
    pub dir: String,
    pub priority: i32,
}

#[derive(Serialize)]
pub struct FullStatusResponse {
    pub status: FullStatus,
}

#[derive(Serialize)]
pub struct FullStatus {
    pub paused: bool,
    pub pause_int: String,
    pub remaining_quota: String,
    pub have_quota: bool,
    pub speed: String,
    pub diskspace1: String,
    pub diskspace2: String,
    pub diskspacetotal1: String,
    pub diskspacetotal2: String,
}

#[derive(Serialize)]
pub struct QueueResponse {
    pub queue: Queue,
}

#[derive(Serialize)]
pub struct Queue {
    pub paused: bool,
    pub slots: Vec<QueueSlot>,
    pub speed: String,
    pub speedlimit: String,
    pub mb: String,
    pub mbleft: String,
    pub noofslots: usize,
    pub noofslots_total: usize,
    pub start: u32,
    pub limit: u32,
}

#[derive(Serialize)]
pub struct QueueSlot {
    pub nzo_id: String,
    pub filename: String,
    pub status: String,
    pub cat: String,
    pub mb: String,
    pub mbleft: String,
    pub size: String,
    pub sizeleft: String,
    pub percentage: String,
    pub priority: String,
    pub script: String,
    pub timeleft: String,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub history: History,
}

#[derive(Serialize)]
pub struct History {
    pub slots: Vec<HistorySlot>,
    pub noofslots: usize,
}

#[derive(Serialize)]
pub struct HistorySlot {
    pub nzo_id: String,
    pub name: String,
    pub nzb_name: String,
    pub category: String,
    pub status: String,
    pub storage: String,
    pub bytes: i64,
    pub download_time: i64,
    pub fail_message: String,
    pub script_line: String,
}

#[derive(Serialize)]
pub struct AddUrlResponse {
    pub status: bool,
    pub nzo_ids: Vec<String>,
}
