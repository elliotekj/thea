use crate::markdown;
use crate::models::Page;
use crate::{CONFIG, TEMPLATES};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;
use tera::Context;
use walkdir::WalkDir;
use yaml_rust::{Yaml, YamlLoader};

pub fn build_hashmap() -> HashMap<String, Page> {
    let mut hashmap = HashMap::new();
    let content_path = CONFIG.get_str("content.path").unwrap();
    let walker = WalkDir::new(content_path);

    for entry in walker {
        let entry = entry.unwrap();

        if entry.file_type().is_dir() {
            continue;
        }

        let page = match parse_file_at(entry.path()) {
            Ok(page) => page,
            Err(e) => {
                error!("For: {} - {:?}", entry.path().display(), e);
                continue;
            }
        };

        hashmap.insert(page.slug.clone(), page);
    }

    if let Ok(static_includes) = CONFIG.get_array("content.static_includes") {
        for entry in static_includes.into_iter() {
            let path_str = entry.into_str().unwrap();
            let path = Path::new(&path_str);
            let page = match parse_static_file(&path) {
                Ok(page) => page,
                Err(e) => {
                    error!("For: {} - {:?}", path.display(), e);
                    continue;
                }
            };

            hashmap.insert(page.slug.clone(), page);
        }
    }

    hashmap
}

fn parse_file_at(path: &Path) -> Result<Page, IoError> {
    let file_contents = fs::read_to_string(path)?;
    let (fm_start, fm_end, content_start) = find_frontmatter(&file_contents)?;
    let frontmatter = &file_contents[fm_start..fm_end];
    let frontmatter_as_yaml = parse_frontmatter(frontmatter)?;
    let content = &file_contents[content_start..];
    let extension_str = path.extension().unwrap().to_str().unwrap();

    let parsed_content = match extension_str {
        "md" => markdown::from(content),
        _ => {
            return Err(IoError::new(
                ErrorKind::Other,
                "File has an unsupported extension.",
            ))
        }
    };

    let mut page = Page {
        title: frontmatter_as_yaml["title"].as_str().unwrap().to_string(),
        slug: frontmatter_as_yaml["slug"].as_str().unwrap().to_string(),
        content: parsed_content,
        rendered: None,
    };

    let html = render_html(&page)?;
    page.rendered = Some(html);

    Ok(page)
}

fn find_frontmatter(content: &str) -> Result<(usize, usize, usize), IoError> {
    let err = IoError::new(ErrorKind::Other, "Failed to find frontmatter.");

    match content.starts_with("---\n") {
        true => {
            let fm = &content[4..];
            let fm_end = match fm.find("---\n") {
                Some(i) => i,
                None => return Err(err),
            };
            Ok((4, fm_end + 4, fm_end + 8))
        }
        false => return Err(err),
    }
}

fn parse_frontmatter(frontmatter: &str) -> Result<Yaml, IoError> {
    YamlLoader::load_from_str(frontmatter)
        .map(|mut yaml_vec| yaml_vec.pop().unwrap())
        .map_err(|_| IoError::new(ErrorKind::Other, "Failed to parse frontmatter as YAML."))
}

fn render_html(page: &Page) -> Result<String, IoError> {
    let context = Context::from_serialize(page).unwrap();

    match TEMPLATES.render("index.html", &context) {
        Ok(html) => Ok(html),
        Err(e) => {
            let mut cause = e.source();

            while let Some(e) = cause {
                error!("Reason: {}", e);
                cause = e.source();
            }

            Err(IoError::new(ErrorKind::Other, e.to_string()))
        }
    }
}

fn parse_static_file(path: &Path) -> Result<Page, IoError> {
    let file_contents = fs::read_to_string(path)?;
    let filename = path.file_name().unwrap().to_str().unwrap().to_string();
    let mut slug = path.display().to_string();

    if slug.chars().next() != Some('/') {
        slug = format!("/{}", slug);
    }

    Ok(Page {
        title: filename,
        slug: slug,
        content: file_contents.clone(),
        rendered: Some(file_contents),
    })
}
