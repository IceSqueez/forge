use std::borrow::Cow;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AssetSource, ElementId, IntoElement, Pixels, Rgba, SharedString,
    Styled, Transformation, percentage, svg,
};

macro_rules! tabler_icons {
    ($($variant:ident => $file:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($variant),+
        }

        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$variant),+];

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Icon::$variant => include_bytes!(concat!("../assets/icons/tabler/", $file))),+
                }
            }

            pub fn path(self) -> &'static str {
                match self {
                    $(Icon::$variant => concat!("tabler/", $file)),+
                }
            }
        }
    };
}

tabler_icons! {
    Home => "home.svg",
    Clock => "clock.svg",
    History => "history.svg",
    Bolt => "bolt.svg",
    Terminal => "terminal-2.svg",
    Broadcast => "broadcast.svg",
    LayoutGrid => "layout-grid.svg",
    Volume => "volume.svg",
    Music => "music.svg",
    Speakerphone => "speakerphone.svg",
    MoodCrazyHappy => "mood-crazy-happy.svg",
    Sparkles => "sparkles.svg",
    UserPlus => "user-plus.svg",
    WaveSine => "wave-sine.svg",
    WaveSawTool => "wave-saw-tool.svg",
    HandClick => "hand-click.svg",
    Ripple => "ripple.svg",
    FileCode => "file-code.svg",
    Server => "server.svg",
    Notebook => "notebook.svg",
    Settings => "settings.svg",
    Users => "users.svg",
    MessageCircle => "message-circle.svg",
    Globe => "globe.svg",
    Activity => "activity.svg",
    Variable => "variable.svg",
    ChevronDown => "chevron-down.svg",
    ChevronUp => "chevron-up.svg",
    ChevronRight => "chevron-right.svg",
    ChevronLeft => "chevron-left.svg",
    Calendar => "calendar.svg",
    Download => "download.svg",
    Plus => "plus.svg",
    Repeat => "repeat.svg",
    Eye => "eye.svg",
    EyeOff => "eye-off.svg",
    BellOff => "bell-off.svg",
    Copy => "copy.svg",
    Pencil => "pencil.svg",
    Edit => "edit.svg",
    Dice => "dice.svg",
    FileText => "file-text.svg",
    Refresh => "refresh.svg",
    AlertTriangle => "alert-triangle.svg",
    AlertCircle => "alert-circle.svg",
    Heartbeat => "heartbeat.svg",
    Lock => "lock.svg",
    X => "x.svg",
    CircleCheck => "circle-check.svg",
    Check => "check.svg",
    InfoCircle => "info-circle.svg",
    Keyboard => "keyboard.svg",
    Folder => "folder.svg",
    FolderOpen => "folder-open.svg",
    Photo => "photo.svg",
    ExternalLink => "external-link.svg",
    PlayerPause => "player-pause.svg",
    PlayerPlay => "player-play.svg",
    PlayerPlayFilled => "player-play-filled.svg",
    PlayerSkipForward => "player-skip-forward.svg",
    PlayerStop => "player-stop.svg",
    TestPipe => "test-pipe.svg",
    Eraser => "eraser.svg",
    Trash => "trash.svg",
    DotsVertical => "dots-vertical.svg",
    CircleDashed => "circle-dashed.svg",
    Loader2 => "loader-2.svg",
    CircleX => "circle-x.svg",
    ArrowBackUp => "arrow-back-up.svg",
    Star => "star.svg",
    StarFilled => "star-filled.svg",
    TargetArrow => "target-arrow.svg",
    Flag => "flag.svg",
    LayoutSidebar => "layout-sidebar.svg",
    Search => "search.svg",
    MoodSmile => "mood-smile.svg",
    Send => "send.svg",
    Plug => "plug.svg",
    PlugConnected => "plug-connected.svg",
    ChartLine => "chart-line.svg",
    ArrowRight => "arrow-right.svg",
    Diamond => "diamond.svg",
    ArrowUp => "arrow-up.svg",
    ArrowDown => "arrow-down.svg",
    ArrowBarUp => "arrow-bar-up.svg",
    ArrowBarDown => "arrow-bar-down.svg",
    CircleCheckFilled => "circle-check-filled.svg",
    Circle => "circle.svg",
    Coin => "coin.svg",
    Pin => "pin.svg",
    Code => "code.svg",
    Stack2 => "stack-2.svg",
    Message2Share => "message-2-share.svg",
    Piano => "piano.svg",
    BrandDiscord => "brand-discord.svg",
    Network => "network.svg",
    Cloud => "cloud.svg",
    Cpu => "cpu.svg",
    GripVertical => "grip-vertical.svg",
    Wand => "wand.svg",
    Microphone2 => "microphone-2.svg",
    BrandAws => "brand-aws.svg",
    VolumeOff => "volume-off.svg",
    FilterOff => "filter-off.svg",
    Ban => "ban.svg",
    Replace => "replace.svg",
}

impl Icon {
    pub fn from_name(name: &str) -> Self {
        match name {
            "home" => Icon::Home,
            "clock" => Icon::Clock,
            "history" => Icon::History,
            "bolt" | "lightning" => Icon::Bolt,
            "terminal" | "terminal-2" => Icon::Terminal,
            "broadcast" | "device-desktop" | "record" | "brand-twitch" | "twitch" | "brand-obs"
            | "obs" => Icon::Broadcast,
            "stack-2" | "stack" => Icon::Stack2,
            "code" => Icon::Code,
            "message-2-share" => Icon::Message2Share,
            "piano" => Icon::Piano,
            "brand-discord" | "discord" => Icon::BrandDiscord,
            "network" => Icon::Network,
            "layout-grid" | "grid" | "apps" => Icon::LayoutGrid,
            "volume" | "speaker" => Icon::Volume,
            "music" => Icon::Music,
            "speakerphone" => Icon::Speakerphone,
            "mood-crazy-happy" => Icon::MoodCrazyHappy,
            "sparkles" => Icon::Sparkles,
            "user-plus" => Icon::UserPlus,
            "wave-sine" => Icon::WaveSine,
            "wave-saw-tool" => Icon::WaveSawTool,
            "hand-click" => Icon::HandClick,
            "ripple" => Icon::Ripple,
            "file-code" => Icon::FileCode,
            "file-text" => Icon::FileText,
            "server" => Icon::Server,
            "notebook" | "journal" | "logs" => Icon::Notebook,
            "settings" | "gear" => Icon::Settings,
            "users" | "people" => Icon::Users,
            "message-circle" | "chat" => Icon::MessageCircle,
            "globe" => Icon::Globe,
            "activity" | "rss" => Icon::Activity,
            "variable" | "hash" => Icon::Variable,
            "chevron-down" => Icon::ChevronDown,
            "chevron-up" => Icon::ChevronUp,
            "chevron-right" => Icon::ChevronRight,
            "chevron-left" => Icon::ChevronLeft,
            "calendar" | "calendar-event" | "clock-plus" => Icon::Calendar,
            "send" | "paper-plane" => Icon::Send,
            "mood-smile" | "emoji" | "smile" => Icon::MoodSmile,
            "download" => Icon::Download,
            "plus" => Icon::Plus,
            "repeat" | "replay" | "arrows-shuffle" => Icon::Repeat,
            "eye" => Icon::Eye,
            "eye-off" | "eye-slash" => Icon::EyeOff,
            "bell-off" | "bell-slash" => Icon::BellOff,
            "copy" => Icon::Copy,
            "edit" => Icon::Edit,
            "pencil" => Icon::Pencil,
            "dice" => Icon::Dice,
            "script" => Icon::Code,
            "refresh" => Icon::Refresh,
            "loader" | "loader-2" => Icon::Loader2,
            "alert-triangle" | "warning" => Icon::AlertTriangle,
            "filter-off" | "filter" => Icon::FilterOff,
            "ban" | "forbid" | "prohibited" => Icon::Ban,
            "replace" | "text-replace" => Icon::Replace,
            "alert-circle" => Icon::AlertCircle,
            "heartbeat" => Icon::Heartbeat,
            "lock" | "key" => Icon::Lock,
            "x" | "close" => Icon::X,
            "circle-check" | "check" | "check-circle" | "shield-check" | "circle" => {
                Icon::CircleCheck
            }
            "info-circle" | "info" | "list" | "flag-alert" => Icon::InfoCircle,
            "keyboard" => Icon::Keyboard,
            "folder" => Icon::Folder,
            "folder-open" => Icon::FolderOpen,
            "photo" | "file-image" | "image" | "camera" | "file-photo" => Icon::Photo,
            "external-link" | "external" => Icon::ExternalLink,
            "player-pause" | "pause" => Icon::PlayerPause,
            "player-play" | "play" => Icon::PlayerPlay,
            "player-play-filled" => Icon::PlayerPlayFilled,
            "player-skip-forward" => Icon::PlayerSkipForward,
            "player-stop" => Icon::PlayerStop,
            "test-pipe" => Icon::TestPipe,
            "eraser" => Icon::Eraser,
            "trash" => Icon::Trash,
            "dots-vertical" => Icon::DotsVertical,
            "circle-dashed" => Icon::CircleDashed,
            "circle-x" | "x-circle" => Icon::CircleX,
            "arrow-back-up" | "rotate-ccw" | "undo" | "arrow-counterclockwise" => Icon::ArrowBackUp,
            "star" | "star-fill" => Icon::Star,
            "target-arrow" | "target" => Icon::TargetArrow,
            "flag" | "flag-fill" => Icon::Flag,
            "layout-sidebar"
            | "sidebar"
            | "layout-sidebar-right"
            | "layout-sidebar-right-collapse" => Icon::LayoutSidebar,
            "search" | "magnifier" | "find" => Icon::Search,
            "plug" | "outlet" => Icon::Plug,
            "plug-connected" => Icon::PlugConnected,
            "chart-line" | "chart" | "graph" | "line-chart" => Icon::ChartLine,
            "arrow-right" => Icon::ArrowRight,
            "diamond" => Icon::Diamond,
            "coin" | "coins" | "bits" => Icon::Coin,
            "pin" | "pinned" | "thumbtack" => Icon::Pin,
            "arrow-up" => Icon::ArrowUp,
            "arrow-down" => Icon::ArrowDown,
            "arrow-bar-up" | "arrow-to-top" => Icon::ArrowBarUp,
            "arrow-bar-down" | "arrow-to-bottom" => Icon::ArrowBarDown,
            "cloud" => Icon::Cloud,
            "cpu" | "chip" | "processor" => Icon::Cpu,
            "grip-vertical" | "grip" => Icon::GripVertical,
            "wand" | "magic" => Icon::Wand,
            "microphone-2" | "microphone" => Icon::Microphone2,
            "brand-aws" | "aws" => Icon::BrandAws,
            "volume-off" | "mute" => Icon::VolumeOff,
            _ => Icon::InfoCircle,
        }
    }
}

/// Register once on the `Application` via `with_assets`, or `svg()` can't resolve an [`Icon::path`].
pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Icon::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|icon_path| icon_path.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

/// `color` fully drives the tint regardless of the paint declared inside the SVG.
pub fn icon(icon: Icon, size: Pixels, color: Rgba) -> impl IntoElement {
    svg()
        .flex_none()
        .size(size)
        .path(icon.path())
        .text_color(color)
}

/// Continuously rotates `glyph`. Each live instance needs a distinct `id`, or gpui shares one animation clock across them.
pub fn spinner(
    id: impl Into<ElementId>,
    glyph: Icon,
    size: Pixels,
    color: Rgba,
) -> impl IntoElement {
    svg()
        .flex_none()
        .size(size)
        .path(glyph.path())
        .text_color(color)
        .with_animation(
            id.into(),
            Animation::new(Duration::from_millis(1200)).repeat(),
            |el, delta| el.with_transformation(Transformation::rotate(percentage(delta))),
        )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use gpui::AssetSource;

    use super::{Icon, IconAssets};

    #[test]
    fn from_name_collapses_aliases_and_falls_back_to_info_circle() {
        for (name, expected) in [
            ("home", Icon::Home),
            ("bolt", Icon::Bolt),
            ("lightning", Icon::Bolt),
            ("x", Icon::X),
            ("close", Icon::X),
            ("edit", Icon::Edit),
            ("gear", Icon::Settings),
            ("", Icon::InfoCircle),
            ("definitely-not-a-real-icon", Icon::InfoCircle),
        ] {
            assert_eq!(Icon::from_name(name), expected, "from_name({name:?})");
        }
    }

    #[test]
    fn load_resolves_known_icon_path_to_bytes() {
        let loaded = IconAssets.load(Icon::Home.path()).unwrap();
        assert!(
            matches!(loaded, Some(bytes) if !bytes.is_empty()),
            "known icon path did not resolve to non-empty bytes"
        );
    }

    #[test]
    fn load_returns_none_for_unknown_path() {
        let loaded = IconAssets.load("tabler/does-not-exist.svg").unwrap();
        assert!(loaded.is_none());
    }
}
