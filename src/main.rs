extern crate actix_rt;
extern crate actix_web;
extern crate config;
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

mod content;
mod markdown;
mod models;

use crate::models::Page;
use actix_web::http::StatusCode;
use actix_web::Result as AppResult;
use actix_web::{guard, middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use config::{Config, File as ConfigFile};
use std::collections::HashMap;
use std::io::Result as IoResult;
use std::{env, process};
use tera::Tera;

lazy_static! {
    pub static ref CONFIG: Config = build_config();
    static ref CONTENT: HashMap<String, Page> = content::build_hashmap();
    pub static ref TEMPLATES: Tera = build_templates();
}

fn build_config() -> Config {
    let mut config = Config::default();
    config.set_default("content.path", "content").unwrap();
    config.merge(ConfigFile::with_name("Config")).unwrap();
    config
}

fn build_templates() -> Tera {
    match Tera::new("templates/**/*") {
        Ok(t) => t,
        Err(e) => {
            error!("Template error(s): {}", e);
            process::exit(1);
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
