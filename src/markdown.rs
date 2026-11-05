use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

use crate::link::{is_safe_href, is_safe_web_href};

const BLOCKED_LINK_DESTINATION: &str = "#blocked-unsafe-link";

pub fn render_markdown(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options).filter_map(|event| match event {
        Event::Html(html) | Event::InlineHtml(html) => safe_inline_html(html).map(Event::InlineHtml),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Link {
            link_type,
            dest_url: sanitize_link_destination(dest_url),
            title,
            id,
        })),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Image {
            link_type,
            dest_url: sanitize_image_destination(dest_url),
            title,
            id,
        })),
        other => Some(other),
    });

    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

fn sanitize_link_destination(destination: CowStr<'_>) -> CowStr<'_> {
    if is_safe_href(&destination) {
        destination
    } else {
        CowStr::Borrowed(BLOCKED_LINK_DESTINATION)
    }
}

fn sanitize_image_destination(destination: CowStr<'_>) -> CowStr<'_> {
    if is_safe_web_href(&destination) {
        destination
    } else {
        CowStr::Borrowed("")
    }
}

fn safe_inline_html(html: CowStr<'_>) -> Option<CowStr<'_>> {
    matches!(
        html.as_ref(),
        "<strong>"
            | "</strong>"
            | "<em>"
            | "</em>"
            | "<del>"
            | "</del>"
            | "<u>"
            | "</u>"
            | "<mark>"
            | "</mark>"
            | "<sup>"
            | "</sup>"
            | "<sub>"
            | "</sub>"
    )
    .then_some(html)
}
