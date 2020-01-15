use crate::CONFIG;
use pulldown_cmark::CowStr;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

lazy_static! {
    pub static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref THEME_SET: ThemeSet = ThemeSet::load_defaults();
}

#[derive(Debug, Default, Clone)]
pub struct CodeBlockOpen {
    pub lang: Option<String>,
    pub filename: Option<String>,
    pub highlights: Option<Vec<usize>>,
}

pub fn parse_codeblock_open<'a>(info: CowStr<'a>) -> CodeBlockOpen {
    let parts = info.split_whitespace().collect::<Vec<&str>>();
    let mut codeblock_open: CodeBlockOpen = Default::default();

    for p in parts {
        if p.contains("=") == false {
            codeblock_open.lang = Some(p.to_string());
        } else if p.starts_with("filename=") {
            let filename = &p[9..];
            codeblock_open.filename = Some(filename.to_string());
        } else if p.starts_with("highlight=") {
            let lines = &p[10..];
            let line_numbers = lines.split(",").collect::<Vec<&str>>();
            let mut highlight = Vec::with_capacity(line_numbers.len());

            for l in line_numbers {
                match l.parse::<usize>() {
                    Ok(line_nr) => highlight.push(line_nr),
                    Err(e) => error!("Error parsing '{}' - {:?}", l, e),
                };
            }

            match highlight.is_empty() {
                true => codeblock_open.highlights = None,
                false => codeblock_open.highlights = Some(highlight),
            }
        }
    }

    codeblock_open
}

pub fn get_highlighter<'a>(codeblock: &CodeBlockOpen) -> HighlightLines<'a> {
    let config_theme = CONFIG.get_str("content.syntax_theme").unwrap();
    let theme = &THEME_SET.themes[&config_theme];

    match codeblock.lang {
        Some(ref lang) => {
            let syntax = SYNTAX_SET
                .find_syntax_by_token(lang)
                .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

            HighlightLines::new(syntax, theme)
        }
        None => HighlightLines::new(SYNTAX_SET.find_syntax_plain_text(), theme),
    }
}
