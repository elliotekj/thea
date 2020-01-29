extern crate actix_rt;
extern crate actix_web;
#[macro_use]
extern crate clap;
extern crate config;
extern crate env_logger;
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
mod settings;
mod watcher;

use crate::content::FileType;
use crate::models::Page;
use actix_files::Files as ActixFiles;
use actix_web::http::header::{CacheControl, CacheDirective, ContentType};
use actix_web::http::header::{ETag, EntityTag, IF_NONE_MATCH};
use actix_web::http::StatusCode;
use actix_web::Result as AppResult;
use actix_web::{guard, middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use config::{Config, Value as ConfigValue};
use std::collections::HashMap;
use std::env;
use std::io::Result as IoResult;
use std::sync::RwLock;

lazy_static! {
    pub static ref SETTINGS: Config = settings::new();
    static ref CONTENT: RwLock<HashMap<String, Page>> = RwLock::new(content::build_hashmap());
    static ref SHOULD_CACHE: bool = should_cache();
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
        None => return unmatched_slug(req.path()).await,
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

async fn unmatched_slug(slug: &str) -> AppResult<HttpResponse> {
    let redirects = SETTINGS.get_table("redirects").unwrap();

    if let Some(redirect) = redirects.get(slug) {
        return redirect_request(redirect.clone()).await;
    }

    not_found_response().await
}

async fn redirect_request(redirect: ConfigValue) -> AppResult<HttpResponse> {
    let redirect_hashmap = redirect.into_table().unwrap();
    let redirect_type = redirect_hashmap.get("redirect_type").unwrap().to_string();
    let redirect_location = redirect_hashmap.get("to").unwrap().to_string();

    let mut res = match redirect_type.as_ref() {
        "permanent" => HttpResponse::PermanentRedirect(),
        "temporary" => HttpResponse::TemporaryRedirect(),
        _ => unreachable!(),
    };

    res.set_header("Location", redirect_location);
    Ok(res.finish())
}

async fn not_found_response() -> AppResult<HttpResponse> {
    let content = CONTENT.read().unwrap();
    let mut res = HttpResponse::build(StatusCode::NOT_FOUND);

    match content.get("/404") {
        Some(four_oh_four) => {
            let html = four_oh_four.meta.rendered.clone().unwrap();
            res.set(ContentType::html());
            Ok(res.body(&html))
        }
        None => {
            res.set(ContentType::plaintext());
            Ok(res.body("404 Not Found."))
        }
    }
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

    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let is_dev_mode = matches.is_present("dev");
    let should_cache = !is_dev_mode;
    env::set_var("THEA_SHOULD_CACHE", should_cache.to_string());

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
