use forge_overlay::preview_page_url;

const PAGE: &str = "http://127.0.0.1:4455/overlays/sub-alert-1/index.html";

#[test]
fn the_preview_flag_joins_the_query_whatever_shape_the_page_url_arrives_in() {
    for (page_url, expected) in [
        (
            PAGE,
            "http://127.0.0.1:4455/overlays/sub-alert-1/index.html?preview=1",
        ),
        (
            "http://127.0.0.1:4455/overlays/sub-alert-1/index.html?scale=2",
            "http://127.0.0.1:4455/overlays/sub-alert-1/index.html?scale=2&preview=1",
        ),
        (
            "http://127.0.0.1:4455/overlays/sub-alert-1/",
            "http://127.0.0.1:4455/overlays/sub-alert-1/?preview=1",
        ),
        (
            "http://127.0.0.1:4455/overlays/sub-alert-1/?scale=2",
            "http://127.0.0.1:4455/overlays/sub-alert-1/?scale=2&preview=1",
        ),
    ] {
        assert_eq!(
            preview_page_url(page_url),
            expected,
            "{page_url} was not asked to preview itself"
        );
    }
}

#[test]
fn a_page_url_carrying_a_fragment_keeps_the_preview_flag_in_its_query() {
    assert_eq!(
        preview_page_url("http://127.0.0.1:4455/overlays/sub-alert-1/index.html#stage"),
        "http://127.0.0.1:4455/overlays/sub-alert-1/index.html?preview=1#stage",
        "a flag written behind the fragment never reaches the page's query"
    );
}

#[test]
fn asking_a_page_url_to_preview_itself_twice_leaves_one_flag() {
    let once = preview_page_url(PAGE);

    assert_eq!(
        preview_page_url(&once),
        once,
        "a second pass appended the flag again"
    );
}
