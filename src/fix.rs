use std::fmt::{Debug, Write};

use regex::Regex;

/// The replacement root URL. This goes in the place of `"https://twitter.com"`.
const REPLACEMENT_STEM: &str = "https://vxtwitter.com";
/// The regex for the link pattern. Matches any space-delimited URL with
/// `twitter` or `x` as the host.



/// Matches all twitter links in a content slice and outputs an iterator with
/// their paths. For example, "https://twitter.com/path" extracts to `["/path"]`.
fn tweet_paths<'a>(
    content: &'a str,
    link_regex: &'a Regex,
) -> impl Iterator<Item = (&'a str, crate::pass::SpoilerTags)> {
    [].into_iter()
}

/// Takes a content slice, extracts all of its Twitter links, and embeds them
/// in masked links. If a link was ||spoilered||, the output link will also have
/// a spoiler tag.
///
/// If there are no changes required to the content, `[None]`
/// is returned.
pub fn fix<'a>(content: &'a str, link_regex: &'a Regex) -> Option<String> {
    let out = tweet_paths(content, link_regex).enumerate().fold(
        String::new(),
        |mut out, (idx, (path, spoiler))| {
            #[inline]
            fn write_link(out: &mut String, idx: usize, path: &str) {
                let _ = write!(
                    out,
                    "[`Tweet #{num}`]({REPLACEMENT_STEM}{path})",
                    num = idx + 1
                );
            }

            if spoiler != SpoilerTags::None {
                let _ = write!(&mut out, "||");
                write_link(&mut out, idx, path);
                let _ = write!(&mut out, "|| ");
            } else {
                write_link(&mut out, idx, path);
            }

            out
        },
    );

    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::{tweet_paths, SpoilerTags};

    #[test]
    fn extract() {
        let link_regex =
            Regex::new("(?:^|\\s)(\\|\\||)https://(?:x|twitter)\\.com(/\\S+)(\\s?\\|\\||)")
                .unwrap();

        assert_eq!(
            tweet_paths(
                "https://x.com/normal/x/link
                https://twitter.com/twitter/link
                ||https://twitter.com/a-spoiler||
                not even a link
                ||https://twitter.com/spoiler/with/space ||
                <https://twitter.com/embed-suppressed-link>",
                &link_regex
            )
            .collect::<Vec<_>>(),
            vec![
                ("/normal/x/link", SpoilerTags::None),
                ("/twitter/link", SpoilerTags::None),
                ("/a-spoiler", SpoilerTags::Spoiler),
                ("/spoiler/with/space", SpoilerTags::Spoiler),
            ],
        );
    }
}
