use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Page {
    pub title: String,
    pub slug: String,
    pub content: String,
    pub rendered: Option<String>,
    pub etag: String,
}
