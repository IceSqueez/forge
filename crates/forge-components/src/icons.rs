use std::borrow::Cow;

use gpui::{AssetSource, IntoElement, Pixels, Rgba, SharedString, Styled, svg};

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

            /// Asset path served by [`IconAssets`] and handed to gpui's `svg()`.
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
    Bolt => "bolt.svg",
    Terminal => "terminal-2.svg",
    Broadcast => "broadcast.svg",
    LayoutGrid => "layout-grid.svg",
    Volume => "volume.svg",
    Music => "music.svg",
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
    Download => "download.svg",
    Plus => "plus.svg",
    Repeat => "repeat.svg",
    Eye => "eye.svg",
    EyeOff => "eye-off.svg",
    Copy => "copy.svg",
    Pencil => "pencil.svg",
    Refresh => "refresh.svg",
    AlertTriangle => "alert-triangle.svg",
    Lock => "lock.svg",
    X => "x.svg",
    CircleCheck => "circle-check.svg",
    InfoCircle => "info-circle.svg",
    Keyboard => "keyboard.svg",
    Folder => "folder.svg",
    FolderOpen => "folder-open.svg",
    Photo => "photo.svg",
    ExternalLink => "external-link.svg",
    PlayerPause => "player-pause.svg",
    PlayerPlay => "player-play.svg",
    Eraser => "eraser.svg",
    DotsVertical => "dots-vertical.svg",
    CircleDashed => "circle-dashed.svg",
    Loader2 => "loader-2.svg",
    CircleX => "circle-x.svg",
    ArrowBackUp => "arrow-back-up.svg",
    Star => "star.svg",
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
}

impl Icon {
    pub fn from_name(name: &str) -> Self {
        match name {
            "home" => Icon::Home,
            "clock" => Icon::Clock,
            "bolt" | "lightning" => Icon::Bolt,
            "terminal" | "terminal-2" => Icon::Terminal,
            "broadcast" | "device-desktop" | "stack-2" | "record" | "brand-twitch" | "twitch"
            | "brand-obs" | "obs" => Icon::Broadcast,
            "layout-grid" | "grid" | "apps" => Icon::LayoutGrid,
            "volume" | "speaker" => Icon::Volume,
            "music" => Icon::Music,
            "file-code" => Icon::FileCode,
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
            "send" | "paper-plane" => Icon::Send,
            "mood-smile" | "emoji" | "smile" => Icon::MoodSmile,
            "download" => Icon::Download,
            "plus" => Icon::Plus,
            "repeat" | "replay" | "arrows-shuffle" => Icon::Repeat,
            "eye" => Icon::Eye,
            "eye-off" | "eye-slash" => Icon::EyeOff,
            "copy" => Icon::Copy,
            "edit" | "pencil" => Icon::Pencil,
            "refresh" => Icon::Refresh,
            "loader" | "loader-2" => Icon::Loader2,
            "alert-triangle" | "warning" => Icon::AlertTriangle,
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
            "eraser" => Icon::Eraser,
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
            _ => Icon::InfoCircle,
        }
    }
}

/// gpui `AssetSource` serving the embedded tabler icon bytes. Register once on
/// the `Application` via `with_assets`; gpui's `svg()` element then resolves an
/// [`Icon::path`] string through `load`.
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

/// Renders a tabler glyph tinted with `color`. gpui rasterizes the SVG to an
/// alpha mask and paints it in the element's text color, so `color` fully drives
/// the tint regardless of the paint declared inside the SVG file.
pub fn icon(icon: Icon, size: Pixels, color: Rgba) -> impl IntoElement {
    svg()
        .flex_none()
        .size(size)
        .path(icon.path())
        .text_color(color)
}
