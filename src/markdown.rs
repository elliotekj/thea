use crate::codeblocks;
use pulldown_cmark::{html as md_html, Options as MdOptions, Parser as MdParser};
use pulldown_cmark::{Event, Tag};
use syntect::easy::HighlightLines;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};

pub fn from(content: &str) -> String {
    let options = MdOptions::all();
    let mut codeblock_open: Option<codeblocks::CodeBlockOpen> = None;
    let mut highlighter: Option<HighlightLines> = None;
    let mut html_output = String::new();

    {
        let events = MdParser::new_ext(content, options)
            .map(|event| match event {
                Event::Start(Tag::CodeBlock(info)) => {
                    codeblock_open = Some(codeblocks::parse_codeblock_open(info));
                    let cbo = codeblock_open.clone().unwrap();
                    highlighter = Some(codeblocks::get_highlighter(&cbo));
                    let mut html = String::from("");

                    if let Some(filename) = &cbo.filename {
                        html += &format!("<span class=\"pre-filename\">{}</span>", filename);
                    };

                    match &cbo.lang {
                        Some(lang) => html += &format!("<pre class=\"lang-{}\"><code>", lang),
                        None => html += "<pre><code>",
                    };

                    Event::Html(html.into())
                }

                Event::Text(text) => {
                    if let Some(ref mut highlighter) = highlighter {
                        let cbo = codeblock_open.clone().unwrap();
                        let hltd = highlighter.highlight(&text, &codeblocks::SYNTAX_SET);

                        let mut html =
                            styled_line_to_highlighted_html(&hltd, IncludeBackground::No)
                                .split("\n")
                                .enumerate()
                                .map(|(i, line)| {
                                    let line_nb = i + 1;
                                    let mut final_line = String::from("<div class=\"line");

                                    if let Some(highlighted_lines) = &cbo.highlights {
                                        if highlighted_lines.contains(&line_nb) {
                                            final_line += " line-highlight\">";
                                        } else {
                                            final_line += "\">";
                                        }
                                    } else {
                                        final_line += "\">";
                                    }

                                    final_line +=
                                        &format!("<span class=\"line-nb\">{}</span>", line_nb);

                                    final_line += line;

                                    if let Some(highlighted_lines) = &cbo.highlights {
                                        if highlighted_lines.contains(&line_nb) {
                                            final_line += "</span>";
                                        }
                                    }

                                    final_line += "</div>";
                                    final_line
                                })
                                .collect::<Vec<String>>();

                        html.pop();
                        return Event::Html(html.join("").into());
                    }

                    Event::Text(text)
                }

                Event::End(Tag::CodeBlock(_)) => {
                    codeblock_open = None;
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
