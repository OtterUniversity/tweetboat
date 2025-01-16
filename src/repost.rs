use std::sync::Arc;

use regex::{Captures, Regex};
use twilight_model::id::{marker::MessageMarker, Id};

use crate::State;

/// Pattern that matches urls which have been transformed by [pass]
const URL_REGEX: &str = "\\[`\\w+`\\]\\((?P<url>.+)\\)";

/// Counts the number of times that a url has been seen
fn check_repost(state: &Arc<State>, embed_url: &str) -> usize {
    let existing_posts = state.seen.read().unwrap().search_by_value(embed_url);
    existing_posts.len()
}

/// Takes a string and inserts the number of times it has been seen
pub fn add_repost_counts(state: &Arc<State>, reply_id: Id<MessageMarker>, content: &str) -> Option<String> {
    let seen = state.seen.read().unwrap().get_entry(reply_id);
    if let Some(_seen) = seen {
        return Some(content.to_owned());
    }
    let mut new_content = content.to_owned();
    for url in find_urls(content) {
        let times = check_repost(state, url);
        let token = state.seen.write().unwrap().file_pending(reply_id);
        if let Some(token) = token {
            if times == 0 {
                state.seen.write().unwrap().insert(token, url.to_owned());
                continue;
            }
            new_content = add_repost_count(content, times);
            state.seen.write().unwrap().insert(token, url.to_owned());
        }
    }
    if new_content == content {
        return None
    }
    Some(new_content)
}

/// Returns a vector of URLs that exist in a string
fn find_urls(content: &str) -> Vec<&str> {
    Regex::new(URL_REGEX)
        .unwrap()
        .captures_iter(content)
        .map(|e| e.name("url").unwrap().as_str())
        .collect()
}

/// Adds the number of times a link has been reposted to a string
fn add_repost_count(content: &str, repost_count: usize) -> String {
    let regex = Regex::new(URL_REGEX).unwrap();
    regex.replace(content, |caps: &Captures|
        format!("{} Posted {} time(s) ", &caps[0], repost_count)
    ).into_owned()
}