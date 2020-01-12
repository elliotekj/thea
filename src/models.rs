use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub page_type: String,
    pub title: Option<String>,
    pub date: Option<String>,
    pub slug: String,
    pub content: String,
    pub rendered: Option<String>,
    #[serde(skip_serializing)]
    pub meta: PageMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageMeta {
    pub layout: Option<String>,
    pub etag: String,
}

pub struct ConfigPageType {
    pub ttype: String,
    pub path: String,
    pub default_template: String,
}
