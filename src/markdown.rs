use pulldown_cmark::{html as md_html, Options as MdOptions, Parser as MdParser};

pub fn from(content: &str) -> String {
    let options = MdOptions::all();
    let parser = MdParser::new_ext(content, options);

    let mut html_output = String::new();
    md_html::push_html(&mut html_output, parser);

    html_output
}
