use pulldown_cmark::{html as md_html, Options as MdOptions, Parser as MdParser};
use pulldown_cmark::{CowStr, Event, Tag};

#[derive(Debug, Default)]
struct CodeBlockOpen {
    lang: Option<String>,
    filename: Option<String>,
    highlights: Option<Vec<usize>>,
}

pub fn from(content: &str) -> String {
    let options = MdOptions::all();
    let mut html_output = String::new();

    {
        let events = MdParser::new_ext(content, options)
            .map(|event| match event {
                Event::Start(Tag::CodeBlock(info)) => {
                    let codeblock_open = parse_codeblock_open(info);
                    let mut html = String::from("");

                    if let Some(filename) = codeblock_open.filename {
                        html += &format!("<span class=\"pre-filename\">{}</span>", filename);
                    };

                    match codeblock_open.lang {
                        Some(lang) => html += &format!("<pre class=\"lang-{}\"><code>", lang),
                        None => html += "<pre><code>",
                    };

                    Event::Html(html.into())
                }
                _ => event,
            })
            .collect::<Vec<_>>();

        md_html::push_html(&mut html_output, events.into_iter());
    }

    html_output
}

fn parse_codeblock_open<'a>(info: CowStr<'a>) -> CodeBlockOpen {
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
