use tweetboat::config::Config;
use tweetboat::pass::SpoilerTags;

#[test]
fn standard_passes() {
    let config: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
    let mut extracted = config.passes.iter().flat_map(|pass| {
        pass.extract(
            "
                These are just some random test urls.
                - https://x.com/rustbeltenjoyer/status/1776056709737320578
                - ||https://www.instagram.com/p/C5W2QwZrt-Z/ ||
                - https://www.tiktok.com/t/ZPRTX3AwH/
            ",
        )
    });

    // NOTE: emitted items are in order of extractor, not appearance

    assert_eq!(
        extracted.next(),
        Some((
            "/rustbeltenjoyer/status/1776056709737320578",
            SpoilerTags::None
        ))
    );
    assert_eq!(
        extracted.next(),
        Some((
            "/p/C5W2QwZrt-Z/",
            SpoilerTags::Spoiler
        ))
    );
    assert_eq!(
        extracted.next(),
        Some(("/t/ZPRTX3AwH/", SpoilerTags::None))
    );

    assert!(extracted.next().is_none());
}
