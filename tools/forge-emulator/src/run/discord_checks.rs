use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, Verdict};
use crate::discord::{FakeDiscord, RecordedPost};
use crate::scenario::DiscordPost;

const POST_METHOD: &str = "POST";

pub(crate) fn post_matches(post: &RecordedPost, expected: &DiscordPost) -> bool {
    let accepted = (200..300).contains(&post.status);
    let content_matches = expected.content_contains.as_deref().is_none_or(|needle| {
        post.content
            .as_deref()
            .is_some_and(|content| content.contains(needle))
    });
    let mentions_match = expected.mention_parse.as_ref().is_none_or(|kinds| {
        let mut kinds = kinds.clone();
        kinds.sort();
        post.mention_parse() == Some(kinds)
    });
    accepted
        && post.method == POST_METHOD
        && post.webhook.as_deref() == Some(expected.webhook.as_str())
        && content_matches
        && mentions_match
}

pub(crate) async fn observe_discord_post(
    fake: &FakeDiscord,
    from: usize,
    deadline: Instant,
    expected: &DiscordPost,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("discord post", remaining, |posts| {
            posts
                .iter()
                .skip(from)
                .any(|post| post_matches(post, expected))
                .then_some(())
        })
        .await
        .is_ok();
    let posts: Vec<RecordedPost> = fake.posts().into_iter().skip(from).collect();
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoDiscordPost {
            observed: posts.len(),
        })
    };
    let evidence = posts.into_iter().take(EVIDENCE_LIMIT).collect();
    (verdict, Evidence::Discord(evidence))
}
