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
extern crate pulldown_cmark;
extern crate serde;
extern crate shellexpand;
extern crate syntect;
extern crate tera;
extern crate walkdir;
extern crate yaml_rust;

mod codeblocks;
mod content;
mod markdown;
mod models;

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
use std::io::Result as IoResult;
use std::path::Path;
use std::{env, process};
use tera::Tera;

lazy_static! {
    pub static ref CONFIG: Config = build_config();
    static ref CONTENT: HashMap<String, Page> = content::build_hashmap();
    pub static ref TEMPLATES: Tera = build_templates();
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
    config.set_default("content.syntax_theme", "InspiredGitHub").unwrap();
    config.set_default("templates.path", "templates").unwrap();
    config.set_default("write_to_disk", false).unwrap();
    config.merge(ConfigFile::with_name("Config")).unwrap();

    let mut base_path_str = config.get_str("base_path").unwrap();

    if base_path_str.starts_with("~") {
        base_path_str = shellexpand::tilde(&base_path_str).to_string();
    }

    let base_path = Path::new(&base_path_str);
    let path_fields = ["content.path", "templates.path"];

    for c in &path_fields {
        let path_str = config.get_str(c).unwrap();
        let path = base_path.join(path_str);
        let absolute_path_str = path.to_str().unwrap();
        config.set(c, absolute_path_str).unwrap();
    }

    config
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

fn should_cache() -> bool {
    let thea_cache_str = env::var("THEA_SHOULD_CACHE").unwrap_or("true".into());
    thea_cache_str.parse::<bool>().unwrap_or(true)
}

async fn catchall(req: HttpRequest) -> AppResult<HttpResponse> {
    let page = match CONTENT.get(req.path()) {
        Some(page) => page,
        None => return not_found_response().await,
    };

    let page_etag = EntityTag::strong(page.meta.etag.clone());
    if resource_was_modified(&req, &page_etag) == false {
        return Ok(HttpResponse::NotModified().finish());
    }

    let html = page.rendered.clone().unwrap();
    let mut res = HttpResponse::build(StatusCode::OK);
    let slug_parts = page.slug.split(".").collect::<Vec<&str>>();

    if (slug_parts.len() > 1) && slug_parts.last().is_some() {
        match slug_parts.last().unwrap() {
            &"css" => res.set(ContentType(mime::TEXT_CSS_UTF_8)),
            &"js" => res.set(ContentType(mime::APPLICATION_JAVASCRIPT_UTF_8)),
            &"json" => res.set(ContentType::json()),
            &"xml" => res.set(ContentType(mime::TEXT_XML)),
            _ => res.set(ContentType::plaintext()),
        };
    } else {
        res.set(ContentType::html());
    }

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
