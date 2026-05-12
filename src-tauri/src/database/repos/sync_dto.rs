use serde::{Deserialize, Serialize};

/// 标签同步数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsSyncData {
    pub tags: Vec<TagSyncEntry>,
    pub associations: Vec<TagAssocSyncEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSyncEntry {
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagAssocSyncEntry {
    pub content_hash: String,
    pub tag_name: String,
    #[serde(default)]
    pub sort_order: i64,
}