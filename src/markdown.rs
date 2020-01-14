use crate::codeblocks;
use pulldown_cmark::{html as md_html, Options as MdOptions, Parser as MdParser};
use pulldown_cmark::{Event, Tag};
use syntect::easy::HighlightLines;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};

pub fn from(content: &str) -> String {
    let options = MdOptions::all();
    let mut highlighter: Option<HighlightLines> = None;
    let mut html_output = String::new();

    {
        let events = MdParser::new_ext(content, options)
            .map(|event| match event {
                Event::Start(Tag::CodeBlock(info)) => {
                    let codeblock_open = codeblocks::parse_codeblock_open(info);
                    highlighter = Some(codeblocks::get_highlighter(&codeblock_open));
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

                Event::Text(text) => {
                    if let Some(ref mut highlighter) = highlighter {
                        let hltd = highlighter.highlight(&text, &codeblocks::SYNTAX_SET);
                        let html = styled_line_to_highlighted_html(&hltd, IncludeBackground::No);
                        return Event::Html(html.into());
                    }

                    Event::Text(text)
                }

                Event::End(Tag::CodeBlock(_)) => {
                    highlighter = None;
                    Event::Html("</code></pre>".into())
                }

                _ => event,
            })
            .collect::<Vec<_>>();

        md_html::push_html(&mut html_output, events.into_iter());
    }

    html_output
}
