use forge_platform_core::PlatformError;
use forge_storage::StorageError;
use thiserror::Error;

use crate::bot_account::BotUnavailable;
use crate::chat::MAX_WHISPER_CHARS;

#[derive(Debug, Error)]
pub enum TwitchError {
    #[error("rate limited")]
    RateLimited,
    #[error("reauth required")]
    ReauthRequired,
    #[error("{0}")]
    Credentials(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("{0}")]
    Transport(String),
    #[error("{0}")]
    UndecodableResponse(String),

    #[error("message exceeds 500-character limit")]
    MessageTooLong,
    #[error("not connected")]
    NotConnected,
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("message dropped by twitch ({code}): {message}")]
    MessageDropped { code: String, message: String },
    #[error("twitch refused the message: {0}")]
    MessageForbidden(String),

    #[error("whisper recipient is empty")]
    WhisperRecipientEmpty,
    #[error("whisper message is empty")]
    WhisperMessageEmpty,
    #[error("whisper message exceeds {MAX_WHISPER_CHARS}-character limit")]
    WhisperTooLong,
    #[error("recipient does not accept whispers")]
    RecipientRejectsWhispers,

    #[error("bot not connected ({0})")]
    BotNotConnected(BotUnavailable),
    #[error("the bot account has no verified phone number, so Twitch refused the whisper")]
    BotPhoneUnverified,
    #[error("bot is not a moderator - make it one in Integrations -> Twitch")]
    BotNotModerator,
    #[error("Twitch refused: {0}")]
    PermissionDenied(String),
    #[error("{0}")]
    SendRefused(String),
    #[error("shoutout cooldown: try again in {retry_after_secs}s")]
    ShoutoutCooldown { retry_after_secs: u64 },
    #[error("Twitch rate limit or cooldown reached; try again later")]
    ModerationThrottled,
    #[error("{0}")]
    SendCredentials(String),
    #[error("no twitch credentials stored")]
    NotSignedIn,

    #[error("this is your streamer account; sign in as the bot account instead")]
    SameAsBroadcaster,
    #[error("bot sign-in was not started")]
    BotLoginNotStarted,
    #[error("bot sign-in cancelled")]
    BotLoginCancelled,
    #[error("bot sign-in denied")]
    BotLoginDenied,
    #[error("bot sign-in code expired")]
    BotLoginExpired,
    #[error(transparent)]
    Platform(PlatformError),
    #[error("credential storage failed: {0}")]
    CredentialStorage(StorageError),
    #[error("the bot credentials could not be cleared: {0}")]
    BotCredentialsNotCleared(PlatformError),

    #[error("no bot account is signed in")]
    BotNotSignedIn,
    #[error("sign in to the streamer account again to grant moderator management")]
    BroadcasterReauthRequired,
    #[error("the bot account is banned in this channel; unban it first")]
    BotBanned,
    #[error("the bot account is a VIP; remove its VIP status first")]
    BotIsVip,
    #[error("Twitch refused: {0}")]
    ModeratorRefused(String),
    #[error("{0}")]
    ModeratorUnavailable(String),

    #[error("scope missing; re-authentication required")]
    ScopeMissing,
    #[error("access token rejected")]
    TokenRejected,

    #[error("{0}")]
    DialSocket(String),
    #[error("connect timed out")]
    DialConnectTimedOut,
    #[error("timed out before session_welcome")]
    WelcomeTimedOut,
    #[error("session_welcome missing session.id")]
    WelcomeWithoutSession,
    #[error("closed before session_welcome")]
    ClosedBeforeWelcome,
}
