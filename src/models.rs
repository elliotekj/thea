use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Page {
    pub title: String,
    pub slug: String,
    pub content: String,
    pub rendered_html: Option<String>,
}
