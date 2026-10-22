use forge_components::tr;

const ANNOUNCEMENT_COLOR_KEY: &str = "color";

pub(crate) fn announcement_color_label(key: &str, option: &str) -> Option<String> {
    if key != ANNOUNCEMENT_COLOR_KEY {
        return None;
    }
    let label = match option {
        "primary" => tr!("announcement_color_primary"),
        "blue" => tr!("announcement_color_blue"),
        "green" => tr!("announcement_color_green"),
        "orange" => tr!("announcement_color_orange"),
        "purple" => tr!("announcement_color_purple"),
        _ => return None,
    };
    Some(label)
}
