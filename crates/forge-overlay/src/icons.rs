const CATEGORY_TOKEN_SUPPORT: &str = "support";
const CATEGORY_TOKEN_CELEBRATION: &str = "celebration";
const CATEGORY_TOKEN_COMMUNITY: &str = "community";
const CATEGORY_TOKEN_STREAM: &str = "stream";
const CATEGORY_TOKEN_PLAY: &str = "play";
const CATEGORY_TOKEN_GOALS: &str = "goals";
const CATEGORY_TOKEN_NATURE: &str = "nature";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconCategory {
    Support,
    Celebration,
    Community,
    Stream,
    Play,
    Goals,
    Nature,
}

impl IconCategory {
    pub const ALL: &'static [IconCategory] = &[
        Self::Support,
        Self::Celebration,
        Self::Community,
        Self::Stream,
        Self::Play,
        Self::Goals,
        Self::Nature,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Support => CATEGORY_TOKEN_SUPPORT,
            Self::Celebration => CATEGORY_TOKEN_CELEBRATION,
            Self::Community => CATEGORY_TOKEN_COMMUNITY,
            Self::Stream => CATEGORY_TOKEN_STREAM,
            Self::Play => CATEGORY_TOKEN_PLAY,
            Self::Goals => CATEGORY_TOKEN_GOALS,
            Self::Nature => CATEGORY_TOKEN_NATURE,
        }
    }
}

impl std::fmt::Display for IconCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuratedIcon {
    pub name: &'static str,
    pub category: IconCategory,
    pub words: &'static [&'static str],
    source: &'static [u8],
}

impl CuratedIcon {
    pub fn bytes(&self) -> &'static [u8] {
        self.source
    }

    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_ascii_lowercase();
        needle.is_empty()
            || self.name.contains(&needle)
            || self.category.as_str().contains(&needle)
            || self.words.iter().any(|word| word.contains(&needle))
    }
}

pub fn curated_icon(name: &str) -> Option<&'static CuratedIcon> {
    CURATED_ICONS.iter().find(|icon| icon.name == name)
}

pub const CURATED_ICONS: &[CuratedIcon] = &[
    CuratedIcon {
        name: "heart",
        category: IconCategory::Support,
        words: &["love", "like"],
        source: include_bytes!("../assets/icons/heart.svg"),
    },
    CuratedIcon {
        name: "heart-filled",
        category: IconCategory::Support,
        words: &["love", "like"],
        source: include_bytes!("../assets/icons/heart-filled.svg"),
    },
    CuratedIcon {
        name: "heart-handshake",
        category: IconCategory::Support,
        words: &["care", "charity"],
        source: include_bytes!("../assets/icons/heart-handshake.svg"),
    },
    CuratedIcon {
        name: "hearts",
        category: IconCategory::Support,
        words: &["love"],
        source: include_bytes!("../assets/icons/hearts.svg"),
    },
    CuratedIcon {
        name: "gift",
        category: IconCategory::Support,
        words: &["present", "donation"],
        source: include_bytes!("../assets/icons/gift.svg"),
    },
    CuratedIcon {
        name: "gift-card",
        category: IconCategory::Support,
        words: &["present", "voucher"],
        source: include_bytes!("../assets/icons/gift-card.svg"),
    },
    CuratedIcon {
        name: "crown",
        category: IconCategory::Support,
        words: &["king", "queen", "royal", "vip"],
        source: include_bytes!("../assets/icons/crown.svg"),
    },
    CuratedIcon {
        name: "diamond",
        category: IconCategory::Support,
        words: &["gem", "jewel"],
        source: include_bytes!("../assets/icons/diamond.svg"),
    },
    CuratedIcon {
        name: "coin",
        category: IconCategory::Support,
        words: &["money", "bits", "currency"],
        source: include_bytes!("../assets/icons/coin.svg"),
    },
    CuratedIcon {
        name: "coins",
        category: IconCategory::Support,
        words: &["money", "bits"],
        source: include_bytes!("../assets/icons/coins.svg"),
    },
    CuratedIcon {
        name: "cash",
        category: IconCategory::Support,
        words: &["money", "dollar"],
        source: include_bytes!("../assets/icons/cash.svg"),
    },
    CuratedIcon {
        name: "moneybag",
        category: IconCategory::Support,
        words: &["money", "donation", "tip"],
        source: include_bytes!("../assets/icons/moneybag.svg"),
    },
    CuratedIcon {
        name: "pig-money",
        category: IconCategory::Support,
        words: &["money", "savings"],
        source: include_bytes!("../assets/icons/pig-money.svg"),
    },
    CuratedIcon {
        name: "receipt",
        category: IconCategory::Support,
        words: &["bill", "invoice"],
        source: include_bytes!("../assets/icons/receipt.svg"),
    },
    CuratedIcon {
        name: "wallet",
        category: IconCategory::Support,
        words: &["money"],
        source: include_bytes!("../assets/icons/wallet.svg"),
    },
    CuratedIcon {
        name: "award",
        category: IconCategory::Support,
        words: &["prize"],
        source: include_bytes!("../assets/icons/award.svg"),
    },
    CuratedIcon {
        name: "medal",
        category: IconCategory::Support,
        words: &["prize", "winner"],
        source: include_bytes!("../assets/icons/medal.svg"),
    },
    CuratedIcon {
        name: "medal-2",
        category: IconCategory::Support,
        words: &["prize", "winner"],
        source: include_bytes!("../assets/icons/medal-2.svg"),
    },
    CuratedIcon {
        name: "trophy",
        category: IconCategory::Support,
        words: &["prize", "winner", "champion"],
        source: include_bytes!("../assets/icons/trophy.svg"),
    },
    CuratedIcon {
        name: "rosette",
        category: IconCategory::Support,
        words: &["badge", "prize"],
        source: include_bytes!("../assets/icons/rosette.svg"),
    },
    CuratedIcon {
        name: "certificate",
        category: IconCategory::Support,
        words: &["diploma", "award"],
        source: include_bytes!("../assets/icons/certificate.svg"),
    },
    CuratedIcon {
        name: "ticket",
        category: IconCategory::Support,
        words: &["pass", "raffle"],
        source: include_bytes!("../assets/icons/ticket.svg"),
    },
    CuratedIcon {
        name: "thumb-up",
        category: IconCategory::Support,
        words: &["like", "approve"],
        source: include_bytes!("../assets/icons/thumb-up.svg"),
    },
    CuratedIcon {
        name: "star",
        category: IconCategory::Support,
        words: &["favorite"],
        source: include_bytes!("../assets/icons/star.svg"),
    },
    CuratedIcon {
        name: "star-filled",
        category: IconCategory::Support,
        words: &["favorite"],
        source: include_bytes!("../assets/icons/star-filled.svg"),
    },
    CuratedIcon {
        name: "stars",
        category: IconCategory::Support,
        words: &["favorite"],
        source: include_bytes!("../assets/icons/stars.svg"),
    },
    CuratedIcon {
        name: "sparkles",
        category: IconCategory::Support,
        words: &["shine", "magic"],
        source: include_bytes!("../assets/icons/sparkles.svg"),
    },
    CuratedIcon {
        name: "confetti",
        category: IconCategory::Celebration,
        words: &["party", "celebrate"],
        source: include_bytes!("../assets/icons/confetti.svg"),
    },
    CuratedIcon {
        name: "balloon",
        category: IconCategory::Celebration,
        words: &["party"],
        source: include_bytes!("../assets/icons/balloon.svg"),
    },
    CuratedIcon {
        name: "cake",
        category: IconCategory::Celebration,
        words: &["birthday"],
        source: include_bytes!("../assets/icons/cake.svg"),
    },
    CuratedIcon {
        name: "candle",
        category: IconCategory::Celebration,
        words: &["birthday"],
        source: include_bytes!("../assets/icons/candle.svg"),
    },
    CuratedIcon {
        name: "glass-champagne",
        category: IconCategory::Celebration,
        words: &["drink", "toast", "party"],
        source: include_bytes!("../assets/icons/glass-champagne.svg"),
    },
    CuratedIcon {
        name: "glass-cocktail",
        category: IconCategory::Celebration,
        words: &["drink", "party"],
        source: include_bytes!("../assets/icons/glass-cocktail.svg"),
    },
    CuratedIcon {
        name: "laurel-wreath",
        category: IconCategory::Celebration,
        words: &["victory", "winner"],
        source: include_bytes!("../assets/icons/laurel-wreath.svg"),
    },
    CuratedIcon {
        name: "comet",
        category: IconCategory::Celebration,
        words: &["space"],
        source: include_bytes!("../assets/icons/comet.svg"),
    },
    CuratedIcon {
        name: "rainbow",
        category: IconCategory::Celebration,
        words: &["pride", "colors"],
        source: include_bytes!("../assets/icons/rainbow.svg"),
    },
    CuratedIcon {
        name: "candy",
        category: IconCategory::Celebration,
        words: &["sweet"],
        source: include_bytes!("../assets/icons/candy.svg"),
    },
    CuratedIcon {
        name: "lollipop",
        category: IconCategory::Celebration,
        words: &["sweet", "candy"],
        source: include_bytes!("../assets/icons/lollipop.svg"),
    },
    CuratedIcon {
        name: "mood-happy",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "smile"],
        source: include_bytes!("../assets/icons/mood-happy.svg"),
    },
    CuratedIcon {
        name: "mood-smile",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "smile"],
        source: include_bytes!("../assets/icons/mood-smile.svg"),
    },
    CuratedIcon {
        name: "mood-crazy-happy",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "excited"],
        source: include_bytes!("../assets/icons/mood-crazy-happy.svg"),
    },
    CuratedIcon {
        name: "mood-tongue",
        category: IconCategory::Celebration,
        words: &["face", "emoji"],
        source: include_bytes!("../assets/icons/mood-tongue.svg"),
    },
    CuratedIcon {
        name: "mood-heart",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "love"],
        source: include_bytes!("../assets/icons/mood-heart.svg"),
    },
    CuratedIcon {
        name: "mood-wink",
        category: IconCategory::Celebration,
        words: &["face", "emoji"],
        source: include_bytes!("../assets/icons/mood-wink.svg"),
    },
    CuratedIcon {
        name: "mood-cry",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "sad"],
        source: include_bytes!("../assets/icons/mood-cry.svg"),
    },
    CuratedIcon {
        name: "mood-surprised",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "shock"],
        source: include_bytes!("../assets/icons/mood-surprised.svg"),
    },
    CuratedIcon {
        name: "mood-nerd",
        category: IconCategory::Celebration,
        words: &["face", "emoji", "glasses"],
        source: include_bytes!("../assets/icons/mood-nerd.svg"),
    },
    CuratedIcon {
        name: "mood-kid",
        category: IconCategory::Celebration,
        words: &["face", "emoji"],
        source: include_bytes!("../assets/icons/mood-kid.svg"),
    },
    CuratedIcon {
        name: "user",
        category: IconCategory::Community,
        words: &["person", "viewer"],
        source: include_bytes!("../assets/icons/user.svg"),
    },
    CuratedIcon {
        name: "user-plus",
        category: IconCategory::Community,
        words: &["follow", "join", "new"],
        source: include_bytes!("../assets/icons/user-plus.svg"),
    },
    CuratedIcon {
        name: "user-check",
        category: IconCategory::Community,
        words: &["follow", "verified"],
        source: include_bytes!("../assets/icons/user-check.svg"),
    },
    CuratedIcon {
        name: "user-heart",
        category: IconCategory::Community,
        words: &["follower", "fan"],
        source: include_bytes!("../assets/icons/user-heart.svg"),
    },
    CuratedIcon {
        name: "user-star",
        category: IconCategory::Community,
        words: &["follower", "vip"],
        source: include_bytes!("../assets/icons/user-star.svg"),
    },
    CuratedIcon {
        name: "users",
        category: IconCategory::Community,
        words: &["viewers", "people"],
        source: include_bytes!("../assets/icons/users.svg"),
    },
    CuratedIcon {
        name: "users-group",
        category: IconCategory::Community,
        words: &["community", "viewers"],
        source: include_bytes!("../assets/icons/users-group.svg"),
    },
    CuratedIcon {
        name: "friends",
        category: IconCategory::Community,
        words: &["community", "people"],
        source: include_bytes!("../assets/icons/friends.svg"),
    },
    CuratedIcon {
        name: "message",
        category: IconCategory::Community,
        words: &["chat"],
        source: include_bytes!("../assets/icons/message.svg"),
    },
    CuratedIcon {
        name: "message-circle",
        category: IconCategory::Community,
        words: &["chat"],
        source: include_bytes!("../assets/icons/message-circle.svg"),
    },
    CuratedIcon {
        name: "message-2",
        category: IconCategory::Community,
        words: &["chat"],
        source: include_bytes!("../assets/icons/message-2.svg"),
    },
    CuratedIcon {
        name: "messages",
        category: IconCategory::Community,
        words: &["chat"],
        source: include_bytes!("../assets/icons/messages.svg"),
    },
    CuratedIcon {
        name: "message-heart",
        category: IconCategory::Community,
        words: &["chat", "love"],
        source: include_bytes!("../assets/icons/message-heart.svg"),
    },
    CuratedIcon {
        name: "message-star",
        category: IconCategory::Community,
        words: &["chat", "highlight"],
        source: include_bytes!("../assets/icons/message-star.svg"),
    },
    CuratedIcon {
        name: "speakerphone",
        category: IconCategory::Community,
        words: &["announce", "shout"],
        source: include_bytes!("../assets/icons/speakerphone.svg"),
    },
    CuratedIcon {
        name: "bell",
        category: IconCategory::Community,
        words: &["notify", "alert"],
        source: include_bytes!("../assets/icons/bell.svg"),
    },
    CuratedIcon {
        name: "bell-ringing",
        category: IconCategory::Community,
        words: &["notify", "alert"],
        source: include_bytes!("../assets/icons/bell-ringing.svg"),
    },
    CuratedIcon {
        name: "bell-heart",
        category: IconCategory::Community,
        words: &["notify", "follow"],
        source: include_bytes!("../assets/icons/bell-heart.svg"),
    },
    CuratedIcon {
        name: "at",
        category: IconCategory::Community,
        words: &["mention"],
        source: include_bytes!("../assets/icons/at.svg"),
    },
    CuratedIcon {
        name: "hash",
        category: IconCategory::Community,
        words: &["tag", "channel"],
        source: include_bytes!("../assets/icons/hash.svg"),
    },
    CuratedIcon {
        name: "eye",
        category: IconCategory::Community,
        words: &["watch", "viewers"],
        source: include_bytes!("../assets/icons/eye.svg"),
    },
    CuratedIcon {
        name: "flag",
        category: IconCategory::Community,
        words: &["report", "mark"],
        source: include_bytes!("../assets/icons/flag.svg"),
    },
    CuratedIcon {
        name: "pin",
        category: IconCategory::Community,
        words: &["mark"],
        source: include_bytes!("../assets/icons/pin.svg"),
    },
    CuratedIcon {
        name: "bookmark",
        category: IconCategory::Community,
        words: &["save"],
        source: include_bytes!("../assets/icons/bookmark.svg"),
    },
    CuratedIcon {
        name: "broadcast",
        category: IconCategory::Stream,
        words: &["live", "stream"],
        source: include_bytes!("../assets/icons/broadcast.svg"),
    },
    CuratedIcon {
        name: "antenna",
        category: IconCategory::Stream,
        words: &["signal", "live"],
        source: include_bytes!("../assets/icons/antenna.svg"),
    },
    CuratedIcon {
        name: "video",
        category: IconCategory::Stream,
        words: &["camera", "stream"],
        source: include_bytes!("../assets/icons/video.svg"),
    },
    CuratedIcon {
        name: "camera",
        category: IconCategory::Stream,
        words: &["photo"],
        source: include_bytes!("../assets/icons/camera.svg"),
    },
    CuratedIcon {
        name: "microphone",
        category: IconCategory::Stream,
        words: &["mic", "audio"],
        source: include_bytes!("../assets/icons/microphone.svg"),
    },
    CuratedIcon {
        name: "microphone-2",
        category: IconCategory::Stream,
        words: &["mic", "audio"],
        source: include_bytes!("../assets/icons/microphone-2.svg"),
    },
    CuratedIcon {
        name: "headphones",
        category: IconCategory::Stream,
        words: &["audio"],
        source: include_bytes!("../assets/icons/headphones.svg"),
    },
    CuratedIcon {
        name: "headset",
        category: IconCategory::Stream,
        words: &["audio", "chat"],
        source: include_bytes!("../assets/icons/headset.svg"),
    },
    CuratedIcon {
        name: "music",
        category: IconCategory::Stream,
        words: &["song", "audio"],
        source: include_bytes!("../assets/icons/music.svg"),
    },
    CuratedIcon {
        name: "playlist",
        category: IconCategory::Stream,
        words: &["music", "queue"],
        source: include_bytes!("../assets/icons/playlist.svg"),
    },
    CuratedIcon {
        name: "vinyl",
        category: IconCategory::Stream,
        words: &["music", "record"],
        source: include_bytes!("../assets/icons/vinyl.svg"),
    },
    CuratedIcon {
        name: "radio",
        category: IconCategory::Stream,
        words: &["audio"],
        source: include_bytes!("../assets/icons/radio.svg"),
    },
    CuratedIcon {
        name: "device-tv",
        category: IconCategory::Stream,
        words: &["screen"],
        source: include_bytes!("../assets/icons/device-tv.svg"),
    },
    CuratedIcon {
        name: "device-gamepad-2",
        category: IconCategory::Stream,
        words: &["game", "controller"],
        source: include_bytes!("../assets/icons/device-gamepad-2.svg"),
    },
    CuratedIcon {
        name: "device-gamepad",
        category: IconCategory::Stream,
        words: &["game", "controller"],
        source: include_bytes!("../assets/icons/device-gamepad.svg"),
    },
    CuratedIcon {
        name: "movie",
        category: IconCategory::Stream,
        words: &["film", "video"],
        source: include_bytes!("../assets/icons/movie.svg"),
    },
    CuratedIcon {
        name: "photo",
        category: IconCategory::Stream,
        words: &["image", "picture"],
        source: include_bytes!("../assets/icons/photo.svg"),
    },
    CuratedIcon {
        name: "player-play",
        category: IconCategory::Stream,
        words: &["start"],
        source: include_bytes!("../assets/icons/player-play.svg"),
    },
    CuratedIcon {
        name: "player-pause",
        category: IconCategory::Stream,
        words: &["stop"],
        source: include_bytes!("../assets/icons/player-pause.svg"),
    },
    CuratedIcon {
        name: "player-record",
        category: IconCategory::Stream,
        words: &["rec"],
        source: include_bytes!("../assets/icons/player-record.svg"),
    },
    CuratedIcon {
        name: "volume",
        category: IconCategory::Stream,
        words: &["sound", "audio"],
        source: include_bytes!("../assets/icons/volume.svg"),
    },
    CuratedIcon {
        name: "screen-share",
        category: IconCategory::Stream,
        words: &["stream", "display"],
        source: include_bytes!("../assets/icons/screen-share.svg"),
    },
    CuratedIcon {
        name: "sword",
        category: IconCategory::Play,
        words: &["fight", "battle"],
        source: include_bytes!("../assets/icons/sword.svg"),
    },
    CuratedIcon {
        name: "swords",
        category: IconCategory::Play,
        words: &["fight", "battle", "pvp"],
        source: include_bytes!("../assets/icons/swords.svg"),
    },
    CuratedIcon {
        name: "shield",
        category: IconCategory::Play,
        words: &["defense", "protect"],
        source: include_bytes!("../assets/icons/shield.svg"),
    },
    CuratedIcon {
        name: "shield-star",
        category: IconCategory::Play,
        words: &["defense", "mod"],
        source: include_bytes!("../assets/icons/shield-star.svg"),
    },
    CuratedIcon {
        name: "target",
        category: IconCategory::Play,
        words: &["aim", "goal"],
        source: include_bytes!("../assets/icons/target.svg"),
    },
    CuratedIcon {
        name: "target-arrow",
        category: IconCategory::Play,
        words: &["aim", "goal"],
        source: include_bytes!("../assets/icons/target-arrow.svg"),
    },
    CuratedIcon {
        name: "dice",
        category: IconCategory::Play,
        words: &["random", "luck"],
        source: include_bytes!("../assets/icons/dice.svg"),
    },
    CuratedIcon {
        name: "dice-5",
        category: IconCategory::Play,
        words: &["random", "luck"],
        source: include_bytes!("../assets/icons/dice-5.svg"),
    },
    CuratedIcon {
        name: "puzzle",
        category: IconCategory::Play,
        words: &["game"],
        source: include_bytes!("../assets/icons/puzzle.svg"),
    },
    CuratedIcon {
        name: "chess",
        category: IconCategory::Play,
        words: &["game", "strategy"],
        source: include_bytes!("../assets/icons/chess.svg"),
    },
    CuratedIcon {
        name: "chess-knight",
        category: IconCategory::Play,
        words: &["game", "strategy"],
        source: include_bytes!("../assets/icons/chess-knight.svg"),
    },
    CuratedIcon {
        name: "cards",
        category: IconCategory::Play,
        words: &["game", "poker"],
        source: include_bytes!("../assets/icons/cards.svg"),
    },
    CuratedIcon {
        name: "ghost",
        category: IconCategory::Play,
        words: &["spooky", "halloween"],
        source: include_bytes!("../assets/icons/ghost.svg"),
    },
    CuratedIcon {
        name: "skull",
        category: IconCategory::Play,
        words: &["death", "spooky"],
        source: include_bytes!("../assets/icons/skull.svg"),
    },
    CuratedIcon {
        name: "alien",
        category: IconCategory::Play,
        words: &["space", "ufo"],
        source: include_bytes!("../assets/icons/alien.svg"),
    },
    CuratedIcon {
        name: "robot",
        category: IconCategory::Play,
        words: &["bot"],
        source: include_bytes!("../assets/icons/robot.svg"),
    },
    CuratedIcon {
        name: "wand",
        category: IconCategory::Play,
        words: &["magic"],
        source: include_bytes!("../assets/icons/wand.svg"),
    },
    CuratedIcon {
        name: "crystal-ball",
        category: IconCategory::Play,
        words: &["magic", "fortune"],
        source: include_bytes!("../assets/icons/crystal-ball.svg"),
    },
    CuratedIcon {
        name: "flask",
        category: IconCategory::Play,
        words: &["potion", "science"],
        source: include_bytes!("../assets/icons/flask.svg"),
    },
    CuratedIcon {
        name: "poo",
        category: IconCategory::Play,
        words: &["poop"],
        source: include_bytes!("../assets/icons/poo.svg"),
    },
    CuratedIcon {
        name: "ufo",
        category: IconCategory::Play,
        words: &["alien", "space"],
        source: include_bytes!("../assets/icons/ufo.svg"),
    },
    CuratedIcon {
        name: "planet",
        category: IconCategory::Play,
        words: &["space"],
        source: include_bytes!("../assets/icons/planet.svg"),
    },
    CuratedIcon {
        name: "meteor",
        category: IconCategory::Play,
        words: &["space"],
        source: include_bytes!("../assets/icons/meteor.svg"),
    },
    CuratedIcon {
        name: "bomb",
        category: IconCategory::Play,
        words: &["explode"],
        source: include_bytes!("../assets/icons/bomb.svg"),
    },
    CuratedIcon {
        name: "flame",
        category: IconCategory::Play,
        words: &["fire", "hot"],
        source: include_bytes!("../assets/icons/flame.svg"),
    },
    CuratedIcon {
        name: "bolt",
        category: IconCategory::Play,
        words: &["lightning", "power"],
        source: include_bytes!("../assets/icons/bolt.svg"),
    },
    CuratedIcon {
        name: "rocket",
        category: IconCategory::Play,
        words: &["launch", "boost"],
        source: include_bytes!("../assets/icons/rocket.svg"),
    },
    CuratedIcon {
        name: "flag-3",
        category: IconCategory::Goals,
        words: &["goal", "finish"],
        source: include_bytes!("../assets/icons/flag-3.svg"),
    },
    CuratedIcon {
        name: "chart-bar",
        category: IconCategory::Goals,
        words: &["stats", "graph"],
        source: include_bytes!("../assets/icons/chart-bar.svg"),
    },
    CuratedIcon {
        name: "chart-line",
        category: IconCategory::Goals,
        words: &["stats", "graph", "trend"],
        source: include_bytes!("../assets/icons/chart-line.svg"),
    },
    CuratedIcon {
        name: "trending-up",
        category: IconCategory::Goals,
        words: &["growth", "stats"],
        source: include_bytes!("../assets/icons/trending-up.svg"),
    },
    CuratedIcon {
        name: "progress",
        category: IconCategory::Goals,
        words: &["bar", "loading"],
        source: include_bytes!("../assets/icons/progress.svg"),
    },
    CuratedIcon {
        name: "hourglass",
        category: IconCategory::Goals,
        words: &["time", "wait"],
        source: include_bytes!("../assets/icons/hourglass.svg"),
    },
    CuratedIcon {
        name: "clock",
        category: IconCategory::Goals,
        words: &["time"],
        source: include_bytes!("../assets/icons/clock.svg"),
    },
    CuratedIcon {
        name: "alarm",
        category: IconCategory::Goals,
        words: &["time", "reminder"],
        source: include_bytes!("../assets/icons/alarm.svg"),
    },
    CuratedIcon {
        name: "stopwatch",
        category: IconCategory::Goals,
        words: &["timer", "speedrun"],
        source: include_bytes!("../assets/icons/stopwatch.svg"),
    },
    CuratedIcon {
        name: "calendar",
        category: IconCategory::Goals,
        words: &["date", "schedule"],
        source: include_bytes!("../assets/icons/calendar.svg"),
    },
    CuratedIcon {
        name: "calendar-event",
        category: IconCategory::Goals,
        words: &["date", "schedule"],
        source: include_bytes!("../assets/icons/calendar-event.svg"),
    },
    CuratedIcon {
        name: "checklist",
        category: IconCategory::Goals,
        words: &["tasks", "todo"],
        source: include_bytes!("../assets/icons/checklist.svg"),
    },
    CuratedIcon {
        name: "circle-check",
        category: IconCategory::Goals,
        words: &["done", "ok"],
        source: include_bytes!("../assets/icons/circle-check.svg"),
    },
    CuratedIcon {
        name: "circle-x",
        category: IconCategory::Goals,
        words: &["fail", "error"],
        source: include_bytes!("../assets/icons/circle-x.svg"),
    },
    CuratedIcon {
        name: "check",
        category: IconCategory::Goals,
        words: &["done", "ok"],
        source: include_bytes!("../assets/icons/check.svg"),
    },
    CuratedIcon {
        name: "x",
        category: IconCategory::Goals,
        words: &["close", "fail"],
        source: include_bytes!("../assets/icons/x.svg"),
    },
    CuratedIcon {
        name: "plus",
        category: IconCategory::Goals,
        words: &["add", "new"],
        source: include_bytes!("../assets/icons/plus.svg"),
    },
    CuratedIcon {
        name: "info-circle",
        category: IconCategory::Goals,
        words: &["help", "about"],
        source: include_bytes!("../assets/icons/info-circle.svg"),
    },
    CuratedIcon {
        name: "alert-triangle",
        category: IconCategory::Goals,
        words: &["warning", "caution"],
        source: include_bytes!("../assets/icons/alert-triangle.svg"),
    },
    CuratedIcon {
        name: "lock",
        category: IconCategory::Goals,
        words: &["private", "secure"],
        source: include_bytes!("../assets/icons/lock.svg"),
    },
    CuratedIcon {
        name: "key",
        category: IconCategory::Goals,
        words: &["unlock", "access"],
        source: include_bytes!("../assets/icons/key.svg"),
    },
    CuratedIcon {
        name: "sun",
        category: IconCategory::Nature,
        words: &["day", "light"],
        source: include_bytes!("../assets/icons/sun.svg"),
    },
    CuratedIcon {
        name: "moon",
        category: IconCategory::Nature,
        words: &["night", "dark"],
        source: include_bytes!("../assets/icons/moon.svg"),
    },
    CuratedIcon {
        name: "cloud",
        category: IconCategory::Nature,
        words: &["weather"],
        source: include_bytes!("../assets/icons/cloud.svg"),
    },
    CuratedIcon {
        name: "snowflake",
        category: IconCategory::Nature,
        words: &["winter", "cold"],
        source: include_bytes!("../assets/icons/snowflake.svg"),
    },
    CuratedIcon {
        name: "leaf",
        category: IconCategory::Nature,
        words: &["nature", "plant"],
        source: include_bytes!("../assets/icons/leaf.svg"),
    },
    CuratedIcon {
        name: "flower",
        category: IconCategory::Nature,
        words: &["plant"],
        source: include_bytes!("../assets/icons/flower.svg"),
    },
    CuratedIcon {
        name: "clover",
        category: IconCategory::Nature,
        words: &["luck"],
        source: include_bytes!("../assets/icons/clover.svg"),
    },
    CuratedIcon {
        name: "paw",
        category: IconCategory::Nature,
        words: &["pet", "animal"],
        source: include_bytes!("../assets/icons/paw.svg"),
    },
    CuratedIcon {
        name: "cat",
        category: IconCategory::Nature,
        words: &["pet", "animal"],
        source: include_bytes!("../assets/icons/cat.svg"),
    },
    CuratedIcon {
        name: "dog",
        category: IconCategory::Nature,
        words: &["pet", "animal"],
        source: include_bytes!("../assets/icons/dog.svg"),
    },
    CuratedIcon {
        name: "fish",
        category: IconCategory::Nature,
        words: &["animal"],
        source: include_bytes!("../assets/icons/fish.svg"),
    },
    CuratedIcon {
        name: "butterfly",
        category: IconCategory::Nature,
        words: &["animal"],
        source: include_bytes!("../assets/icons/butterfly.svg"),
    },
    CuratedIcon {
        name: "feather",
        category: IconCategory::Nature,
        words: &["bird", "light"],
        source: include_bytes!("../assets/icons/feather.svg"),
    },
    CuratedIcon {
        name: "coffee",
        category: IconCategory::Nature,
        words: &["drink"],
        source: include_bytes!("../assets/icons/coffee.svg"),
    },
    CuratedIcon {
        name: "pizza",
        category: IconCategory::Nature,
        words: &["food"],
        source: include_bytes!("../assets/icons/pizza.svg"),
    },
    CuratedIcon {
        name: "cookie",
        category: IconCategory::Nature,
        words: &["food", "sweet"],
        source: include_bytes!("../assets/icons/cookie.svg"),
    },
    CuratedIcon {
        name: "beer",
        category: IconCategory::Nature,
        words: &["drink"],
        source: include_bytes!("../assets/icons/beer.svg"),
    },
    CuratedIcon {
        name: "ice-cream",
        category: IconCategory::Nature,
        words: &["food", "sweet"],
        source: include_bytes!("../assets/icons/ice-cream.svg"),
    },
    CuratedIcon {
        name: "apple",
        category: IconCategory::Nature,
        words: &["fruit", "food"],
        source: include_bytes!("../assets/icons/apple.svg"),
    },
    CuratedIcon {
        name: "world",
        category: IconCategory::Nature,
        words: &["globe", "earth"],
        source: include_bytes!("../assets/icons/world.svg"),
    },
    CuratedIcon {
        name: "globe",
        category: IconCategory::Nature,
        words: &["world", "earth"],
        source: include_bytes!("../assets/icons/globe.svg"),
    },
    CuratedIcon {
        name: "home",
        category: IconCategory::Nature,
        words: &["house"],
        source: include_bytes!("../assets/icons/home.svg"),
    },
    CuratedIcon {
        name: "map-pin",
        category: IconCategory::Nature,
        words: &["location"],
        source: include_bytes!("../assets/icons/map-pin.svg"),
    },
    CuratedIcon {
        name: "compass",
        category: IconCategory::Nature,
        words: &["direction"],
        source: include_bytes!("../assets/icons/compass.svg"),
    },
];
