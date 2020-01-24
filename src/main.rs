extern crate actix_rt;
extern crate actix_web;
#[macro_use]
extern crate clap;
extern crate config;
extern crate flexi_logger;
extern crate html_minifier;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate mime;
extern crate notify;
extern crate pulldown_cmark;
extern crate serde;
extern crate syntect;
extern crate tera;
extern crate walkdir;
extern crate yaml_rust;

mod codeblocks;
mod content;
mod markdown;
mod models;
mod watcher;

use crate::content::FileType;
use crate::models::Page;
use actix_files::Files as ActixFiles;
use actix_web::http::header::{CacheControl, CacheDirective, ContentType};
use actix_web::http::header::{ETag, EntityTag, IF_NONE_MATCH, LOCATION};
use actix_web::http::StatusCode;
use actix_web::Result as AppResult;
use actix_web::{guard, middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use config::{Config, File as ConfigFile};
use flexi_logger::{opt_format, Logger as FlexiLogger};
use std::collections::HashMap;
use std::env;
use std::io::Result as IoResult;
use std::sync::RwLock;

lazy_static! {
    pub static ref CONFIG: Config = build_config();
    static ref CONTENT: RwLock<HashMap<String, Page>> = RwLock::new(content::build_hashmap());
    static ref SHOULD_CACHE: bool = should_cache();
}

fn setup_logger(is_in_dev_mode: bool) {
    let logger = FlexiLogger::with_env_or_str("info");

    if is_in_dev_mode {
        logger.start().unwrap();
    } else {
        logger
            .log_to_file()
            .directory("logs")
            .format(opt_format)
            .start()
            .unwrap();
    }
}

fn build_config() -> Config {
    let mut config = Config::default();
    config.set_default("content.path", "content").unwrap();
    config
        .set_default("content.syntax_theme", "InspiredGitHub")
        .unwrap();
    config.set_default("templates.path", "templates").unwrap();
    config.set_default("write_to_disk", false).unwrap();
    config.merge(ConfigFile::with_name("Config")).unwrap();

    let pwd = env::current_dir().unwrap();
    let path_fields = ["content.path", "templates.path"];

    for path_field in &path_fields {
        let path_str = config.get_str(path_field).unwrap();
        let mut path_buf = pwd.clone();
        path_buf.push(path_str);
        let path_buf_string = path_buf.as_path().to_str().unwrap();
        config.set(path_field, path_buf_string).unwrap();
    }

    config
}

fn should_cache() -> bool {
    let thea_cache_str = env::var("THEA_SHOULD_CACHE").unwrap_or("true".into());
    thea_cache_str.parse::<bool>().unwrap_or(true)
}

pub fn rebuild_site() {
    let mut new_hashmap = content::build_hashmap();

    {
        let existing_hashmap = CONTENT.read().unwrap();

        for (slug, page) in &mut new_hashmap {
            match existing_hashmap.get(slug) {
                Some(existing_page) => {
                    if existing_page.meta.rendered == page.meta.rendered {
                        *page = existing_page.clone();
                    } else {
                        info!("Updated page: {}", slug);
                    }
                }
                None => info!("New page: {}", slug),
            };
        }
    }

    let mut content_write_lock = CONTENT.write().unwrap();
    *content_write_lock = new_hashmap;

    info!("Regenerated the HashMap.");
}

async fn catchall(req: HttpRequest) -> AppResult<HttpResponse> {
    let content = CONTENT.read().unwrap();

    let page = match content.get(req.path()) {
        Some(page) => page,
        None => return not_found_response().await,
    };

    let page_etag = EntityTag::strong(page.meta.etag.clone());
    if resource_was_modified(&req, &page_etag) == false {
        return Ok(HttpResponse::NotModified().finish());
    }

    let html = page.meta.rendered.clone().unwrap();
    let mut res = HttpResponse::build(StatusCode::OK);

    match content::get_filetype(&page.slug) {
        FileType::Html => res.set(ContentType::html()),
        FileType::Css => res.set(ContentType(mime::TEXT_CSS_UTF_8)),
        FileType::Js => res.set(ContentType(mime::APPLICATION_JAVASCRIPT_UTF_8)),
        FileType::Json => res.set(ContentType::json()),
        FileType::Xml => res.set(ContentType(mime::TEXT_XML)),
        FileType::Txt => res.set(ContentType::plaintext()),
    };

    if *SHOULD_CACHE {
        res.set(ETag(page_etag));
        res.set(CacheControl(vec![CacheDirective::MaxAge(900u32)]));
    }

    Ok(res.body(&html))
}

async fn not_found_response() -> AppResult<HttpResponse> {
    Ok(HttpResponse::Found()
        .header(LOCATION, "/404")
        .finish()
        .into_body())
}

fn resource_was_modified(req: &HttpRequest, page_etag: &EntityTag) -> bool {
    match req.headers().get(IF_NONE_MATCH) {
        Some(header) => {
            let mut req_etag_str = header.to_str().unwrap();
            req_etag_str = &req_etag_str[1..req_etag_str.len() - 1];
            let req_etag = EntityTag::strong(req_etag_str.to_string());
            page_etag.strong_ne(&req_etag)
        }
        None => true,
    }
}

#[actix_rt::main]
async fn main() -> IoResult<()> {
    let matches = clap_app!(thea =>
        (version: crate_version!())
        (author: "Elliot Jackson <elliot@elliotekj.com")
        (about: crate_description!())
        (@arg dev: -d --dev "Runs thea in web development mode")
        (@arg PORT: -p --port +takes_value "Sets the port thea starts on"))
    .get_matches();

    let is_dev_mode = matches.is_present("dev");
    let should_cache = !is_dev_mode;
    env::set_var("THEA_SHOULD_CACHE", should_cache.to_string());
    setup_logger(is_dev_mode);

    // Force the initialization of CONTENT so the first request after startup isn't delayed.
    lazy_static::initialize(&CONTENT);

    watcher::watch_files();

    let port = matches.value_of("PORT").unwrap_or("8765");
    let url = format!("127.0.0.1:{}", port);

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .service(ActixFiles::new("/static", "./static"))
            .default_service(
                web::resource("").route(web::get().to(catchall)).route(
                    web::route()
                        .guard(guard::Not(guard::Get()))
                        .to(HttpResponse::MethodNotAllowed),
                ),
            )
    })
    .bind(&url)?
    .run()
    .await
}
