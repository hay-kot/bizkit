//! Markdown to mail. Pure: the send job, preview and test sends all render
//! through [`render`], so what the preview shows is what goes out.

use std::fmt::Write as _;

use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, TextMergeStream};
use url::Url;

use crate::api::Attributes;

/// A mail ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rendered {
    pub(crate) subject: String,
    pub(crate) html: String,
    pub(crate) text: String,
}

/// The values merge fields take: `{{ name }}`, `{{ email }}` and
/// `{{ attr.key }}`. A missing name or attribute renders as nothing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Merge<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) email: &'a str,
    pub(crate) attributes: Option<&'a Attributes>,
}

/// What every mail of a list shows around the body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame<'a> {
    pub(crate) list_name: &'a str,
    pub(crate) footer: Option<&'a str>,
    pub(crate) unsubscribe: Option<&'a Url>,
    /// The reader's manage-subscriptions page.
    pub(crate) manage: Option<&'a Url>,
}

/// Renders a broadcast or step: the subject and body with merge fields
/// filled in, the body's markdown as HTML in the list's layout, and the
/// markdown source as the text part.
pub(crate) fn render(
    frame: &Frame<'_>,
    subject: &str,
    body_md: &str,
    merge: &Merge<'_>,
) -> Rendered {
    let body = markdown_html(body_md, merge);
    Rendered {
        subject: one_line(&fill(subject, merge)),
        html: layout(frame, &body),
        text: with_text_footer(frame, fill(body_md, merge).trim_end()),
    }
}

/// The double opt-in mail.
pub(crate) fn confirm(frame: &Frame<'_>, confirm_url: &Url) -> Rendered {
    let list = escape(frame.list_name);
    let url = escape(confirm_url.as_str());
    let body = format!(
        "<p>Please confirm that you want mail from <strong>{list}</strong>.</p>\
         <p><a href=\"{url}\" style=\"display:inline-block;padding:10px 18px;background:#1f2937;\
         color:#ffffff;text-decoration:none;border-radius:6px\">Confirm subscription</a></p>\
         <p>If you didn't ask for this, ignore this mail and you won't hear from us again.</p>"
    );
    let text = format!(
        "Please confirm that you want mail from {}.\n\nConfirm subscription: {confirm_url}\n\n\
         If you didn't ask for this, ignore this mail and you won't hear from us again.",
        frame.list_name
    );
    let frame = Frame { unsubscribe: None, manage: None, ..*frame };
    Rendered {
        subject: one_line(&format!("Confirm your subscription to {}", frame.list_name)),
        html: layout(&frame, &body),
        text: with_text_footer(&frame, &text),
    }
}

/// Replaces each `{{ field }}` with its value. Unknown fields stay as
/// written, so a typo shows in the preview.
fn fill(text: &str, merge: &Merge<'_>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, opened)) = rest.split_once("{{") {
        let Some((field, after)) = opened.split_once("}}") else { break };
        out.push_str(before);
        if let Some(value) = merge_value(field.trim(), merge) {
            out.push_str(&value);
        } else {
            out.push_str("{{");
            out.push_str(field);
            out.push_str("}}");
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn merge_value(field: &str, merge: &Merge<'_>) -> Option<String> {
    match field {
        "name" => Some(merge.name.unwrap_or("").to_owned()),
        "email" => Some(merge.email.to_owned()),
        _ => {
            let key = field.strip_prefix("attr.")?;
            Some(
                merge
                    .attributes
                    .and_then(|attributes| attributes.0.get(key))
                    .map_or_else(String::new, ToString::to_string),
            )
        }
    }
}

/// Header values can't hold line breaks.
fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_owned()
}

fn allowed_link(url: &str, schemes: &[&str]) -> bool {
    Url::parse(url).is_ok_and(|url| schemes.contains(&url.scheme()))
}

/// The body as HTML. Raw HTML becomes escaped text, and links and images
/// with other schemes than http, https (and mailto for links) render as
/// their text, so a body can't put markup or `javascript:` links in the
/// preview or the mail.
#[expect(clippy::wildcard_enum_match_arm, reason = "every other event renders as it is")]
fn markdown_html(body_md: &str, merge: &Merge<'_>) -> String {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let mut dropped_links = Vec::new();
    let mut dropped_images = Vec::new();
    let events =
        TextMergeStream::new(Parser::new_ext(body_md, options)).filter_map(|event| match event {
            Event::Html(html) | Event::InlineHtml(html) => Some(Event::Text(html)),
            Event::Text(text) => Some(Event::Text(CowStr::from(fill(&text, merge)))),
            Event::Start(Tag::Link { link_type, ref dest_url, .. }) => {
                // An email autolink's `dest_url` is the bare address; the
                // HTML writer adds `mailto:` itself.
                let keep = link_type == LinkType::Email
                    || allowed_link(dest_url, &["http", "https", "mailto"]);
                dropped_links.push(!keep);
                keep.then_some(event)
            }
            Event::End(TagEnd::Link) => {
                let dropped = dropped_links.pop().unwrap_or(false);
                (!dropped).then_some(event)
            }
            Event::Start(Tag::Image { ref dest_url, .. }) => {
                let keep = allowed_link(dest_url, &["http", "https"]);
                dropped_images.push(!keep);
                keep.then_some(event)
            }
            Event::End(TagEnd::Image) => {
                let dropped = dropped_images.pop().unwrap_or(false);
                (!dropped).then_some(event)
            }
            other => Some(other),
        });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

/// One table-based layout with inline styles, which mail clients render
/// most alike.
fn layout(frame: &Frame<'_>, body: &str) -> String {
    let list = escape(frame.list_name);
    let mut footer = format!("<p style=\"margin:0 0 8px\">{list}</p>");
    if let Some(text) = frame.footer.filter(|text| !text.trim().is_empty()) {
        let _ignored = write!(
            footer,
            "<p style=\"margin:0 0 8px\">{}</p>",
            escape(text.trim()).replace('\n', "<br>")
        );
    }
    let links: Vec<String> =
        [(frame.unsubscribe, "Unsubscribe"), (frame.manage, "Manage subscriptions")]
            .into_iter()
            .filter_map(|(url, label)| {
                url.map(|url| {
                    format!(
                        "<a href=\"{}\" style=\"color:#6b7280\">{label}</a>",
                        escape(url.as_str())
                    )
                })
            })
            .collect();
    if !links.is_empty() {
        let _ignored = write!(footer, "<p style=\"margin:0\">{}</p>", links.join(" &middot; "));
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></head>\
         <body style=\"margin:0;padding:0;background:#f3f4f6\">\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:#f3f4f6\">\
         <tr><td align=\"center\" style=\"padding:24px 12px\">\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:600px;background:#ffffff;border-radius:8px\">\
         <tr><td style=\"padding:20px 28px;border-bottom:1px solid #e5e7eb;font:600 16px system-ui,sans-serif;color:#111827\">{list}</td></tr>\
         <tr><td style=\"padding:24px 28px;font:15px/1.6 system-ui,sans-serif;color:#111827\">{body}</td></tr>\
         <tr><td style=\"padding:16px 28px;border-top:1px solid #e5e7eb;font:12px/1.5 system-ui,sans-serif;color:#6b7280\">{footer}</td></tr>\
         </table></td></tr></table></body></html>"
    )
}

fn with_text_footer(frame: &Frame<'_>, body: &str) -> String {
    let mut text = format!("{body}\n\n-- \n{}", frame.list_name);
    if let Some(footer) = frame.footer.filter(|footer| !footer.trim().is_empty()) {
        let _ignored = write!(text, "\n{}", footer.trim());
    }
    if let Some(url) = frame.unsubscribe {
        let _ignored = write!(text, "\nUnsubscribe: {url}");
    }
    if let Some(url) = frame.manage {
        let _ignored = write!(text, "\nManage subscriptions: {url}");
    }
    text.push('\n');
    text
}

/// HTML-escapes text for element content and quoted attributes.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::AttributeValue;

    fn unsubscribe() -> Url {
        Url::parse("https://app.example.com/m/mailing/public/unsubscribe?t=abc").unwrap()
    }

    fn manage() -> &'static Url {
        static URL: std::sync::LazyLock<Url> = std::sync::LazyLock::new(|| {
            Url::parse("https://app.example.com/m/mailing/public/manage?t=def").unwrap()
        });
        &URL
    }

    fn frame(url: &Url) -> Frame<'_> {
        Frame {
            list_name: "Newsletter",
            footer: Some("1 Main St\nSpringfield"),
            unsubscribe: Some(url),
            manage: Some(manage()),
        }
    }

    const ADA: Merge<'static> =
        Merge { name: Some("Ada"), email: "ada@example.com", attributes: None };

    #[test]
    fn renders_markdown_in_the_layout() {
        let url = unsubscribe();
        let body = "Hi {{ name }},\n\n**News** for {{email}}:\n\n- one\n- [two](https://example.com/two)\n";
        let mail = render(&frame(&url), "News for {{ name }}", body, &ADA);
        assert_eq!(mail.subject, "News for Ada", "subject");
        insta::assert_snapshot!("html", mail.html);
        insta::assert_snapshot!("text", mail.text);
    }

    #[test]
    fn raw_html_is_escaped_text() {
        let html = markdown_html("<script>alert(1)</script>\n\nhi <b onclick=x>there</b>", &ADA);
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b "), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn merge_values_are_escaped() {
        let merge = Merge {
            name: Some("<img src=x onerror=alert(1)>"),
            email: "a@example.com",
            attributes: None,
        };
        let html = markdown_html("Hi {{ name }}", &merge);
        assert!(html.contains("&lt;img"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn only_web_and_mail_links_survive() {
        let html = markdown_html(
            "[a](javascript:alert(1)) [b](https://example.com) [c](mailto:x@example.com) \
             [d](/relative) ![e](data:image/png;base64,AA) ![f](https://example.com/f.png)",
            &ADA,
        );
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("data:"), "{html}");
        assert!(!html.contains("href=\"/relative\""), "{html}");
        assert!(html.contains("href=\"https://example.com\""), "{html}");
        assert!(html.contains("href=\"mailto:x@example.com\""), "{html}");
        assert!(html.contains("src=\"https://example.com/f.png\""), "{html}");
        assert!(html.contains(">a<") || html.contains("a "), "dropped link keeps its text: {html}");
    }

    #[test]
    fn email_autolinks_keep_their_link() {
        let html = markdown_html("Questions? <support@example.com>", &ADA);
        assert!(html.contains("href=\"mailto:support@example.com\""), "{html}");
    }

    #[test]
    fn subjects_are_one_line() {
        let url = unsubscribe();
        let merge = Merge {
            name: Some("Ada\r\nBcc: x@example.com"),
            email: "a@example.com",
            attributes: None,
        };
        let mail = render(&frame(&url), "Hi {{ name }}", "", &merge);
        assert!(!mail.subject.contains('\n') && !mail.subject.contains('\r'), "{:?}", mail.subject);
    }

    #[test]
    fn fills_attributes() {
        let attributes = Attributes(std::collections::BTreeMap::from([
            ("plan".to_owned(), AttributeValue::Text("<b>Pro</b>".to_owned())),
            ("seats".to_owned(), AttributeValue::Number(5.into())),
            (
                "ratio".to_owned(),
                AttributeValue::Number(serde_json::Number::from_f64(1.5).unwrap()),
            ),
            ("trial".to_owned(), AttributeValue::Bool(false)),
        ]));
        let merge = Merge { attributes: Some(&attributes), ..ADA };
        let text = fill(
            "{{ attr.plan }}/{{attr.seats}}/{{ attr.ratio }}/{{ attr.trial }}/[{{ attr.missing }}]/{{ nope }}/{{ name",
            &merge,
        );
        assert_eq!(text, "<b>Pro</b>/5/1.5/false/[]/{{ nope }}/{{ name", "text part");
        let html = markdown_html("Plan: {{ attr.plan }}", &merge);
        assert!(html.contains("&lt;b&gt;Pro&lt;/b&gt;"), "escaped in html: {html}");
        assert_eq!(fill("{{ attr.plan }}", &ADA), "", "no attributes");
    }

    #[test]
    fn confirm_mail_has_the_link_and_no_unsubscribe() {
        let url = unsubscribe();
        let confirm_url =
            Url::parse("https://app.example.com/m/mailing/public/confirm?t=x").unwrap();
        let mail = confirm(&frame(&url), &confirm_url);
        assert_eq!(mail.subject, "Confirm your subscription to Newsletter", "subject");
        assert!(mail.html.contains("confirm?t=x"), "html link");
        assert!(mail.text.contains("confirm?t=x"), "text link");
        assert!(!mail.text.contains("Unsubscribe"), "no unsubscribe: {}", mail.text);
    }
}
