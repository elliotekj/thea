use crate::markdown;
use crate::models::{ConfigPageType, Page, PageMeta};
use crate::{CONFIG, TEMPLATES};
use config::Value as ConfigValue;
use html_minifier::HTMLMinifier;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;
use tera::{Context as TeraContext, Map as TeraMap, Value as TeraValue};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};
use yaml_rust::{Yaml, YamlLoader};

trait IntoPageType {
    fn into_page_type(&self) -> Option<ConfigPageType>;
}

pub fn build_hashmap() -> HashMap<String, Page> {
    let mut hashmap = HashMap::new();
    let base_content_path = CONFIG.get_str("content.path").unwrap();
    let page_types = get_page_types();

    for pt in &page_types {
        let walk_path = format!("{}/{}", base_content_path, &pt.path);
        let walker = WalkDir::new(walk_path).into_iter();

        for entry in walker {
            let entry = entry.unwrap();
            if is_cachable(&entry) == false {
                continue;
            }

            let default_layout = pt.default_layout.clone();
            let ttype = pt.ttype.clone();

            let page = match parse_file_at(entry.path(), default_layout, ttype) {
                Ok(page) => page,
                Err(e) => {
                    error!("For: {} - {:?}", entry.path().display(), e);
                    continue;
                }
            };

            hashmap.insert(page.slug.clone(), page);
        }
    }

    render_pages(hashmap)
}

fn get_page_types() -> Vec<ConfigPageType> {
    let config_page_types = CONFIG.get_array("content.page_types").unwrap_or_else(|_| {
        error!("Failed while collecting content.page_types.");
        Vec::with_capacity(0)
    });

    let mut page_types = Vec::with_capacity(config_page_types.len());

    for pt in &config_page_types {
        match pt.into_page_type() {
            Some(pt) => page_types.push(pt),
            None => error!("{:?} has missing parameters; skipping", pt),
        }
    }

    page_types
}

fn is_cachable(entry: &DirEntry) -> bool {
    let supported_extensions = ["md", "html", "css", "js", "json", "txt"];

    match entry.path().extension() {
        Some(ext) => supported_extensions.contains(&ext.to_str().unwrap()),
        None => false,
    }
}

impl IntoPageType for ConfigValue {
    fn into_page_type(&self) -> Option<ConfigPageType> {
        let table = self.clone().into_table().unwrap();

        let ttype = match table.get("type") {
            Some(ttype) => ttype.to_string(),
            None => return None,
        };

        let path = match table.get("path") {
            Some(path) => path.to_string(),
            None => return None,
        };

        let default_layout = match table.get("default_layout") {
            Some(default_layout) => default_layout.to_string(),
            None => return None,
        };

        Some(ConfigPageType {
            ttype: ttype,
            path: path,
            default_layout: default_layout,
        })
    }
}

fn parse_file_at(path: &Path, default_layout: String, ttype: String) -> Result<Page, IoError> {
    let file_contents = fs::read_to_string(path)?;
    let (fm_start, fm_end, content_start) = find_frontmatter(&file_contents)?;
    let frontmatter = &file_contents[fm_start..fm_end];
    let frontmatter_as_yaml = parse_frontmatter(frontmatter)?;
    let content = &file_contents[content_start..];
    let extension_str = path.extension().unwrap().to_str().unwrap();

    let parsed_content = match extension_str {
        "md" => markdown::from(content),
        "html" | "css" | "js" | "json" | "txt" => content.to_string(),
        _ => {
            return Err(IoError::new(
                ErrorKind::Other,
                "File has an unsupported extension.",
            ))
        }
    };

    let err = |k| {
        IoError::new(
            ErrorKind::Other,
            format!("Missing required {} key in frontmatter.", k),
        )
    };

    let page_slug = match frontmatter_as_yaml["slug"].as_str() {
        Some(slug) => slug.to_string(),
        None => return Err(err("slug")),
    };

    let page_meta_layout = match frontmatter_as_yaml["layout"].as_str() {
        Some(layout) => layout.to_string(),
        None => default_layout,
    };

    let fm_dump = dump_frontmatter(frontmatter_as_yaml);

    Ok(Page {
        page_type: ttype,
        slug: page_slug,
        fm: fm_dump,
        content: parsed_content,
        rendered: None,
        meta: PageMeta {
            layout: Some(page_meta_layout),
            etag: Uuid::new_v4().to_string(),
        },
    })
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
        false => Err(err),
    }
}

fn parse_frontmatter(frontmatter: &str) -> Result<Yaml, IoError> {
    YamlLoader::load_from_str(frontmatter)
        .map(|mut yaml_vec| yaml_vec.pop().unwrap())
        .map_err(|_| IoError::new(ErrorKind::Other, "Failed to parse frontmatter as YAML."))
}

fn dump_frontmatter(frontmatter: Yaml) -> TeraMap<String, TeraValue> {
    let mut map = TeraMap::new();

    let frontmatter_hashmap = match frontmatter.into_hash() {
        Some(hm) => hm,
        None => return map,
    };

    for (key, value) in &frontmatter_hashmap {
        let key = key.as_str().unwrap().to_string();

        if let Some(value_as_vec) = value.as_vec() {
            let stringified = value_as_vec
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<String>>();

            map.insert(key, TeraValue::from(stringified));
        } else if let Some(value_as_str) = value.as_str() {
            let stringified = value_as_str.to_string();
            map.insert(key, TeraValue::from(stringified));
        }
    }

    map
}

fn render_pages(hashmap: HashMap<String, Page>) -> HashMap<String, Page> {
    let mut final_hashmap: HashMap<String, Page> = HashMap::new();

    let pages_vec = hashmap
        .clone()
        .into_iter()
        .map(|(_, page)| page)
        .collect::<Vec<Page>>();

    let mut context = TeraContext::new();
    context.insert("pages", &pages_vec);
    context.insert("globals", &dump_globals());

    for (key, page) in hashmap.into_iter() {
        let layout = page.meta.layout.clone().unwrap();
        let mut page_context = context.clone();
        page_context.insert("page", &page);

        let html = match render_html(&layout, page_context) {
            Ok(html) => html,
            Err(e) => {
                error!("Failed to render {}: {:?}.", key, e);
                continue;
            }
        };

        let mut final_page = page.clone();
        final_page.rendered = Some(html);
        final_hashmap.insert(key.to_string(), final_page);
    }

    let should_write_to_disk = CONFIG.get_bool("write_to_disk").unwrap();

    if should_write_to_disk {
        write_rendered_to_disk(&final_hashmap);
    }

    final_hashmap
}

fn dump_globals() -> TeraMap<String, TeraValue> {
    let mut map = TeraMap::new();

    let globals_hashmap = match CONFIG.get_table("templates.globals") {
        Ok(gh) => gh,
        Err(_) => return map,
    };

    for (key, value) in globals_hashmap.into_iter() {
        if let Ok(value_as_bool) = value.clone().into_bool() {
            map.insert(key, TeraValue::from(value_as_bool));
        } else if let Ok(value_as_int) = value.clone().into_int() {
            map.insert(key, TeraValue::from(value_as_int));
        } else if let Ok(value_as_str) = value.into_str() {
            let stringified = value_as_str.to_string();
            map.insert(key, TeraValue::from(stringified));
        }
    }

    map
}

fn render_html(layout: &str, context: TeraContext) -> Result<String, IoError> {
    let mut html_minifier = HTMLMinifier::new();

    let html = match TEMPLATES.render(layout, &context) {
        Ok(html) => html,
        Err(e) => {
            let mut cause = e.source();

            while let Some(e) = cause {
                error!("Reason: {}", e);
                cause = e.source();
            }

            return Err(IoError::new(ErrorKind::Other, e.to_string()));
        }
    };

    match html_minifier.digest(html) {
        Ok(()) => (),
        Err(e) => return Err(IoError::new(ErrorKind::Other, e.to_string())),
    };

    Ok(html_minifier.get_html())
}

fn write_rendered_to_disk(hashmap: &HashMap<String, Page>) {
    let mut base_path_str = CONFIG.get_str("base_path").unwrap();

    if base_path_str.starts_with("~") {
        base_path_str = shellexpand::tilde(&base_path_str).to_string();
    }

    let base_path = Path::new(&base_path_str);
    let rendered_path = base_path.join(".rendered");
    let _ = fs::remove_dir_all(&rendered_path);
    let _ = fs::create_dir(&rendered_path);

    for (key, page) in hashmap {
        let mut page_path_buf = match key.starts_with("/") {
            true => rendered_path.join(&key[1..]),
            false => rendered_path.join(key),
        };

        if page_path_buf.extension().is_none() {
            page_path_buf = page_path_buf.join("index.html");
        }

        let page_path = page_path_buf.as_path();
        let parent_dirs = page_path.parent().unwrap();
        let _ = fs::create_dir_all(parent_dirs);

        let mut file = fs::File::create(page_path).unwrap();
        let file_contents = page.rendered.clone().unwrap();
        let _ = file.write_all(file_contents.as_bytes());
    }
}
