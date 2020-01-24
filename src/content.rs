use crate::models::{ConfigPageType, Page, PageMeta};
use crate::{markdown, CONFIG};
use config::Value as ConfigValue;
use html_minifier::HTMLMinifier;
use std::collections::HashMap;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;
use std::process;
use std::{env, fs};
use tera::{Context as TeraContext, Map as TeraMap, Tera, Value as TeraValue};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};
use yaml_rust::{Yaml, YamlLoader};

pub enum FileType {
    Html,
    Xml,
    Css,
    Js,
    Json,
    Txt,
}

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
    let supported_extensions = ["md", "html", "css", "js", "json", "xml", "txt"];

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

pub fn get_filetype(slug: &str) -> FileType {
    let slug_parts = slug.split(".").collect::<Vec<&str>>();

    if (slug_parts.len() > 1) && slug_parts.last().is_some() {
        match slug_parts.last().unwrap() {
            &"css" => FileType::Css,
            &"js" => FileType::Js,
            &"json" => FileType::Json,
            &"xml" => FileType::Xml,
            _ => FileType::Txt,
        }
    } else {
        FileType::Html
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
        "html" | "css" | "js" | "json" | "xml" | "txt" => content.to_string(),
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
        meta: PageMeta {
            etag: Uuid::new_v4().to_string(),
            layout: Some(page_meta_layout),
            rendered: None,
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

fn build_templates() -> Tera {
    let templates_path = CONFIG.get_str("templates.path").unwrap();
    let templates_glob = format!("{}/**/*", templates_path);

    match Tera::new(&templates_glob) {
        Ok(t) => t,
        Err(e) => {
            error!("Template error(s): {}", e);
            process::exit(1);
        }
    }
}

fn render_pages(hashmap: HashMap<String, Page>) -> HashMap<String, Page> {
    let mut final_hashmap: HashMap<String, Page> = HashMap::new();

    let pages_vec = hashmap
        .clone()
        .into_iter()
        .map(|(_, page)| page)
        .collect::<Vec<Page>>();

    let templates = build_templates();
    let mut context = TeraContext::new();
    context.insert("pages", &pages_vec);
    context.insert("globals", &dump_globals());

    for (key, page) in hashmap.into_iter() {
        let rendered = match render_page(page.clone(), &templates, context.clone()) {
            Ok(rendered) => rendered,
            Err(e) => {
                error!("Failed to render {}: {:?}.", key, e);
                continue;
            }
        };

        let mut final_page = page.clone();
        final_page.meta.rendered = Some(rendered);
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

fn render_page(page: Page, templates: &Tera, mut context: TeraContext) -> Result<String, IoError> {
    context.insert("page", &page);

    let mut rendered = match templates.render(&page.meta.layout.unwrap(), &context) {
        Ok(rendered) => rendered,
        Err(e) => {
            let mut cause = e.source();

            while let Some(e) = cause {
                error!("Reason: {}", e);
                cause = e.source();
            }

            return Err(IoError::new(ErrorKind::Other, e.to_string()));
        }
    };

    match get_filetype(&page.slug) {
        FileType::Html | FileType::Xml | FileType::Json | FileType::Js | FileType::Css => {
            let mut minifier = HTMLMinifier::new();

            if let Err(e) = minifier.digest(rendered) {
                return Err(IoError::new(ErrorKind::Other, e.to_string()));
            };

            rendered = minifier.get_html();
        }
        _ => {}
    }

    Ok(rendered)
}

fn write_rendered_to_disk(hashmap: &HashMap<String, Page>) {
    let mut rendered_path = env::current_dir().unwrap();
    rendered_path.push(".rendered");

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
        let file_contents = page.meta.rendered.clone().unwrap();
        let _ = file.write_all(file_contents.as_bytes());
    }
}
