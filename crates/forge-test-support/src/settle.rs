use std::time::Duration;

pub const HANG_GUARD: Duration = Duration::from_secs(90);

const SETTLE_POLL: Duration = Duration::from_millis(5);

pub async fn settle_until(what: &str, mut done: impl FnMut() -> bool) {
    let settled = tokio::time::timeout(HANG_GUARD, async {
        while !done() {
            tokio::time::sleep(SETTLE_POLL).await;
        }
    })
    .await;
    assert!(settled.is_ok(), "never happened: {what}");
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn returns_once_the_condition_turns_true() {
        let polls = Cell::new(0);

        settle_until("the third poll", || {
            polls.set(polls.get() + 1);
            polls.get() == 3
        })
        .await;

        assert_eq!(polls.get(), 3);
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "never happened: the impossible")]
    async fn names_what_never_happened_when_the_guard_expires() {
        settle_until("the impossible", || false).await;
    }
}
