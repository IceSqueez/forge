mod common;
use common::{ALERT_KIND, CHAT_KIND, text, with_kind};

use forge_overlay::config::{AVATAR, PLATFORM, SHOW_AVATARS, USER_ID};
use forge_overlay::{ChatAvatarSlot, OverlayConfig, page_avatar_url, settle_chat_avatar};
use forge_types::{PlatformId, Variant};

const TWITCH_IMAGE: &str =
    "https://static-cdn.jtvnw.net/jtv_user_pictures/a1-profile_image-70x70.png";
const YOUTUBE_IMAGE: &str = "https://yt3.ggpht.com/ytc/AIdro_k=s88-c-k-c0x00ffffff-no-rj";
const KICK_IMAGE: &str =
    "https://files.kick.com/images/user/7/profile_image/conversion/a-thumb.webp";

fn shown() -> OverlayConfig {
    OverlayConfig::from([(SHOW_AVATARS.to_owned(), Variant::Bool(true))])
}

fn row(platform: &str, user_id: &str, avatar: &str) -> OverlayConfig {
    OverlayConfig::from([
        (PLATFORM.to_owned(), text(platform)),
        (USER_ID.to_owned(), text(user_id)),
        (AVATAR.to_owned(), text(avatar)),
    ])
}

fn settle(
    kind_id: &str,
    stored: &OverlayConfig,
    content: &mut OverlayConfig,
) -> Option<ChatAvatarSlot> {
    with_kind(kind_id, |descriptor| {
        settle_chat_avatar(descriptor, stored, content)
    })
}

fn avatar_of(content: &OverlayConfig) -> Option<&str> {
    content.get(AVATAR).and_then(Variant::as_str)
}

#[test]
fn each_platform_accepts_an_image_on_its_own_hosts_verbatim() {
    for (platform, url) in [
        (PlatformId::Twitch, TWITCH_IMAGE),
        (PlatformId::YouTube, YOUTUBE_IMAGE),
        (PlatformId::YouTube, "https://ggpht.com/a.png"),
        (
            PlatformId::YouTube,
            "https://lh3.googleusercontent.com/a/ACg8oc=s88",
        ),
        (PlatformId::Kick, KICK_IMAGE),
        (
            PlatformId::Kick,
            "https://kick.com/img/default-profile-pictures/a.webp",
        ),
        (PlatformId::Twitch, "HTTPS://Static-CDN.jtvnw.net/x.png"),
        (PlatformId::Twitch, "https://static-cdn.jtvnw.net"),
    ] {
        assert_eq!(
            page_avatar_url(platform, url).as_deref(),
            Some(url),
            "{platform:?} refused its own image {url}"
        );
    }
}

#[test]
fn an_image_on_another_platforms_host_is_refused() {
    for (platform, url) in [
        (PlatformId::Twitch, KICK_IMAGE),
        (PlatformId::Twitch, YOUTUBE_IMAGE),
        (PlatformId::YouTube, TWITCH_IMAGE),
        (PlatformId::YouTube, KICK_IMAGE),
        (PlatformId::Kick, TWITCH_IMAGE),
        (PlatformId::Kick, YOUTUBE_IMAGE),
        (PlatformId::Twitch, "https://evil.example/x.png"),
    ] {
        assert_eq!(
            page_avatar_url(platform, url),
            None,
            "{platform:?} accepted {url}"
        );
    }
}

#[test]
fn an_image_not_served_over_https_is_refused() {
    for url in [
        "http://static-cdn.jtvnw.net/x.png",
        "//static-cdn.jtvnw.net/x.png",
        "static-cdn.jtvnw.net/x.png",
        "ftp://static-cdn.jtvnw.net/x.png",
        "javascript:alert(1)//static-cdn.jtvnw.net",
        "data:image/png;base64,AAAA",
        "https:/static-cdn.jtvnw.net/x.png",
        "https",
        "",
    ] {
        assert_eq!(
            page_avatar_url(PlatformId::Twitch, url),
            None,
            "accepted {url:?}"
        );
    }
}

#[test]
fn a_lookalike_host_that_only_ends_in_a_platform_domain_is_refused() {
    for (platform, url) in [
        (PlatformId::Kick, "https://evilkick.com/x.png"),
        (PlatformId::Kick, "https://kick.com.evil.io/x.png"),
        (PlatformId::YouTube, "https://ggpht.com.evil.io/x.png"),
        (PlatformId::YouTube, "https://notggpht.com/x.png"),
        (
            PlatformId::YouTube,
            "https://googleusercontent.com.evil.io/x.png",
        ),
        (
            PlatformId::Twitch,
            "https://static-cdn.jtvnw.net.evil.io/x.png",
        ),
        (PlatformId::Twitch, "https://jtvnw.net/x.png"),
        (
            PlatformId::Twitch,
            "https://evil.io/static-cdn.jtvnw.net/x.png",
        ),
        (PlatformId::Twitch, "https://evil.io?static-cdn.jtvnw.net"),
        (PlatformId::Twitch, "https://evil.io#static-cdn.jtvnw.net"),
    ] {
        assert_eq!(
            page_avatar_url(platform, url),
            None,
            "{platform:?} accepted {url}"
        );
    }
}

#[test]
fn an_image_url_carrying_userinfo_or_a_port_is_refused() {
    for url in [
        "https://user@static-cdn.jtvnw.net/x.png",
        "https://static-cdn.jtvnw.net@evil.io/x.png",
        "https://user:pass@static-cdn.jtvnw.net/x.png",
        "https://static-cdn.jtvnw.net:443/x.png",
        "https://static-cdn.jtvnw.net:8443/x.png",
        "https:///x.png",
        "https://",
    ] {
        assert_eq!(
            page_avatar_url(PlatformId::Twitch, url),
            None,
            "accepted {url}"
        );
    }
}

#[test]
fn an_image_url_carrying_a_character_that_breaks_out_of_markup_is_refused() {
    for injected in [
        "\"", "'", "<", ">", "\\", "`", " ", "\t", "\n", "\r", "\u{0}",
    ] {
        let url = format!("https://static-cdn.jtvnw.net/a{injected}b.png");
        assert_eq!(
            page_avatar_url(PlatformId::Twitch, &url),
            None,
            "accepted {url:?}"
        );
    }
}

#[test]
fn surrounding_whitespace_is_trimmed_off_an_accepted_image() {
    assert_eq!(
        page_avatar_url(PlatformId::Twitch, &format!("  {TWITCH_IMAGE}\n")).as_deref(),
        Some(TWITCH_IMAGE)
    );
}

#[test]
fn an_image_url_is_accepted_up_to_the_length_cap_and_refused_one_past_it() {
    let base = "https://static-cdn.jtvnw.net/";
    let at_cap = format!("{base}{}", "a".repeat(2048 - base.len()));
    let past_cap = format!("{at_cap}a");

    assert_eq!(
        page_avatar_url(PlatformId::Twitch, &at_cap),
        Some(at_cap.clone())
    );
    assert_eq!(page_avatar_url(PlatformId::Twitch, &past_cap), None);
}

#[test]
fn a_non_chat_overlay_is_left_untouched_and_never_asks_for_a_lookup() {
    let mut content = row("twitch", "42", "javascript:alert(1)");
    let before = content.clone();

    let slot = settle(ALERT_KIND, &shown(), &mut content);

    assert_eq!(slot, None);
    assert_eq!(
        content, before,
        "a non-chat overlay had its content rewritten"
    );
}

#[test]
fn a_given_avatar_the_row_platform_does_not_vouch_for_is_blanked() {
    for (platform, avatar) in [
        ("twitch", "http://static-cdn.jtvnw.net/x.png"),
        ("twitch", KICK_IMAGE),
        ("kick", "https://evilkick.com/x.png"),
        ("", TWITCH_IMAGE),
        ("trovo", TWITCH_IMAGE),
        ("twitch", "https://static-cdn.jtvnw.net/\"><script>.png"),
    ] {
        let mut content = row(platform, "42", avatar);

        settle(CHAT_KIND, &OverlayConfig::new(), &mut content);

        assert_eq!(
            avatar_of(&content),
            Some(""),
            "{avatar} on a {platform:?} row reached the page"
        );
    }
}

#[test]
fn a_valid_given_avatar_is_kept_trimmed_and_needs_no_lookup() {
    let mut content = row("twitch", "42", &format!(" {TWITCH_IMAGE} "));

    let slot = settle(CHAT_KIND, &shown(), &mut content);

    assert_eq!(
        slot, None,
        "a row that already carries its avatar asked for a lookup"
    );
    assert_eq!(avatar_of(&content), Some(TWITCH_IMAGE));
}

#[test]
fn no_lookup_is_asked_while_the_overlay_has_avatars_switched_off() {
    for stored in [
        OverlayConfig::new(),
        OverlayConfig::from([(SHOW_AVATARS.to_owned(), Variant::Bool(false))]),
    ] {
        let mut content = row("twitch", "42", "");

        assert_eq!(settle(CHAT_KIND, &stored, &mut content), None);
    }
}

#[test]
fn no_lookup_is_asked_for_a_row_without_a_known_platform_or_a_user_id() {
    for (platform, user_id) in [
        ("", "42"),
        ("trovo", "42"),
        ("twitch", ""),
        ("twitch", "   "),
    ] {
        let mut content = row(platform, user_id, "");

        assert_eq!(
            settle(CHAT_KIND, &shown(), &mut content),
            None,
            "platform {platform:?} with user {user_id:?} asked for a lookup"
        );
    }
}

#[test]
fn a_row_of_a_shown_overlay_asks_for_its_viewers_avatar() {
    let mut content = row("YouTube", " UCviewer ", "");

    assert_eq!(
        settle(CHAT_KIND, &shown(), &mut content),
        Some(ChatAvatarSlot {
            platform: PlatformId::YouTube,
            user_id: "UCviewer".to_owned(),
        })
    );
}

#[test]
fn a_blanked_avatar_still_asks_for_a_lookup_to_replace_it() {
    let mut content = row("kick", "7", "https://evilkick.com/x.png");

    let slot = settle(CHAT_KIND, &shown(), &mut content);

    assert_eq!(
        slot,
        Some(ChatAvatarSlot {
            platform: PlatformId::Kick,
            user_id: "7".to_owned(),
        })
    );
}
