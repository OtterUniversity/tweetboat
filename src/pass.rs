use std::{collections::HashMap, fmt::Write};

use regex::Regex;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
pub struct Pass {
    pub label: String,
    #[serde(deserialize_with = "pass_regex")]
    pub regex: Regex,
    pub stem: String,
    pub keep_query: Option<Vec<String>>,
}

/// An enum representing the spoiler tags on a link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpoilerTags {
    /// The link is not spoilered.
    None,
    /// The link has ||spoiler|| tags.
    Spoiler,
    /// The link is either started or terminated with a spoiler tag, but not
    /// both. When resending a link with mismatched tags, the bot will just
    /// apply full spoilers.
    Mismatched,
}

impl Pass {
    pub fn extract<'a>(
        &'a self,
        content: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a str, SpoilerTags)> {
        self.regex.captures_iter(content).map(|capture| {
            let (_, [sp_open, path, sp_close]) = capture.extract();
            let spoiler_marker = match (!sp_open.is_empty(), !sp_close.is_empty()) {
                (false, false) => SpoilerTags::None,
                (true, true) => SpoilerTags::Spoiler,
                _ => SpoilerTags::Mismatched,
            };

            let (path, query) = path.split_once('?').unwrap_or((path, ""));

            (path, query, spoiler_marker)
        })
    }

    pub fn apply<'a>(&'a self, content: &'a str) -> Option<String> {
        let Self { label, stem, .. } = self;

        let out =
            self.extract(content)
                .fold(String::new(), |mut out, (path, query, spoiler_tags)| {
                    let spoil = spoiler_tags != SpoilerTags::None;

                    let query_string = match &self.keep_query {
                        None => format!("?{query}"),
                        Some(keep) if !keep.is_empty() => filter_query(query, keep),
                        _ => String::new(),
                    };

                    if spoil {
                        let _ = write!(&mut out, "||");
                    }
                    let _ = write!(&mut out, "[`{label}`]({stem}{path}{query_string}) ");
                    if spoil {
                        let _ = write!(&mut out, "|| ");
                    }

                    out
                });

        (!out.is_empty()).then_some(out)
    }

    pub fn apply_all(passes: &[Self], content: &str) -> Option<String> {
        let mut transformed = None;
        for pass in passes {
            if let Some(patched) = pass.apply(content) {
                transformed.get_or_insert(String::new()).push_str(&patched);
            }
        }

        transformed
    }
}

/// Deserializes the regex from a pass entry. This pads out the decoded string
/// with spoiler tags and spacing.
fn pass_regex<'de, D: Deserializer<'de>>(de: D) -> Result<Regex, D::Error> {
    use serde::de::Error as _;

    let core = String::deserialize(de)?;
    Regex::new(&["(?:^|\\s)", "(\\|\\||)", &core, "(/\\S+)", "(\\s?\\|\\||)"].concat())
        .map_err(D::Error::custom)
}

/// Removes all query parameters from a query string except those in the provided list
fn filter_query(qs: &str, keep: &Vec<String>) -> String {
    let query_map: HashMap<_, _> = qs.split('&').filter_map(|p| p.split_once('=')).collect();

    let params = query_map
        .iter()
        .filter_map(|(k, v)| keep.contains(&k.to_string()).then(|| format!("{k}={v}")))
        .collect::<Vec<_>>()
        .join("&");

    format!("?{params}")
}
