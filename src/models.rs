use serde::Serialize;
use tera::{Map as TeraMap, Value as TeraValue};

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub page_type: String,
    pub slug: String,
    pub content: String,
    pub fm: TeraMap<String, TeraValue>,
    #[serde(skip_serializing)]
    pub meta: PageMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageMeta {
    pub etag: String,
    pub layout: Option<String>,
    pub rendered: Option<String>,
}

pub struct ConfigPageType {
    pub ttype: String,
    pub path: String,
    pub default_layout: String,
}
