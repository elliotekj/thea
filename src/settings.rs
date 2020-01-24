use config::{Config, File as ConfigFile};
use std::env;

pub fn new() -> Config {
    let mut config = default();
    merge_files(&mut config);
    expand_paths(&mut config);
    config
}

fn default() -> Config {
    let mut config = Config::default();

    config.set_default("content.path", "content").unwrap();
    config
        .set_default("content.syntax_theme", "InspiredGitHub")
        .unwrap();
    config.set_default("templates.path", "templates").unwrap();
    config.set_default("write_to_disk", false).unwrap();

    config
}

fn merge_files(config: &mut Config) {
    let default_config = ConfigFile::with_name("./config/default").required(true);
    config.merge(default_config).unwrap();

    let mode = env::var("THEA_ENV").unwrap_or("development".into());

    match mode.as_ref() {
        "development" => {
            let c = ConfigFile::with_name("./config/development").required(false);
            config.merge(c).unwrap();
        }
        "production" => {
            let c = ConfigFile::with_name("./config/production").required(false);
            config.merge(c).unwrap();
        }
        _ => warn!("default/development/production are the only valid config file names."),
    };
}

fn expand_paths(config: &mut Config) {
    let pwd = env::current_dir().unwrap();
    let path_fields = ["content.path", "templates.path"];

    for path_field in &path_fields {
        let path_str = config.get_str(path_field).unwrap();
        let mut path_buf = pwd.clone();
        path_buf.push(path_str);
        let path_buf_string = path_buf.as_path().to_str().unwrap();
        config.set(path_field, path_buf_string).unwrap();
    }
}
