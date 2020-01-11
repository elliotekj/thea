extern crate actix_rt;
extern crate actix_web;
extern crate env_logger;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate pulldown_cmark;
extern crate serde;
extern crate tera;
extern crate walkdir;
extern crate yaml_rust;

mod markdown;

use actix_web::http::StatusCode;
use actix_web::Result as AppResult;
use actix_web::{guard, middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use serde::Serialize;
use std::collections::HashMap;
use std::{env, fs, process};
use std::error::Error;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::Path;
use tera::{Context, Tera};
use walkdir::WalkDir;
use yaml_rust::{Yaml, YamlLoader};

#[derive(Debug, Serialize)]
struct Page {
    title: String,
    slug: String,
    content: String,
    rendered_html: Option<String>,
}

lazy_static! {
    static ref CONTENT: HashMap<String, Page> = build_content_hashmap();
    static ref TEMPLATES: Tera = {
        match Tera::new("templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                error!("Template error(s): {}", e);
                process::exit(1);
            }
        }
    };
}

fn build_content_hashmap() -> HashMap<String, Page> {
    let mut hashmap = HashMap::new();
    let walker = WalkDir::new("content");

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
        rendered_html: None,
    };

    let html = render_html(&page)?;
    page.rendered_html = Some(html);

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

async fn catchall(req: HttpRequest) -> AppResult<HttpResponse> {
    let page = match CONTENT.get(req.path()) {
        Some(page) => page,
        None => return not_found_response().await,
    };

    let html = page.rendered_html.clone().unwrap();

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(&html))
}

async fn not_found_response() -> AppResult<HttpResponse> {
    Ok(HttpResponse::build(StatusCode::NOT_FOUND)
        .content_type("text/html; charset=utf-8")
        .body("Not found!"))
}

#[actix_rt::main]
async fn main() -> IoResult<()> {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Force the evaluation of CONTENT so the first request after startup isn't delayed.
    let _ = CONTENT.get("/");

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .default_service(
                web::resource("").route(web::get().to(catchall)).route(
                    web::route()
                        .guard(guard::Not(guard::Get()))
                        .to(HttpResponse::MethodNotAllowed),
                ),
            )
    })
    .bind("127.0.0.1:8765")?
    .run()
    .await
}
