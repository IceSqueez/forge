# YouTube Platform

This page covers everything forge does with YouTube in the current release.

---

## Overview

forge connects to YouTube using Google's OAuth 2.0 device-code flow — no passwords, no manual
token paste. Once connected, forge polls your live broadcast's chat feed and exposes the
following capabilities:

| Area | What's available |
| :--- | :--- |
| **Auth** | Device-code OAuth; refresh token stored locally; auto-refresh 5 min before expiry |
| **Chat read** | Live chat messages, `!command` detection |
| **Monetization triggers** | Super Chat, Super Sticker, new channel members, member milestones |
| **Moderation triggers** | Timeout, permanent ban |
| **Broadcast lifecycle** | Live broadcast started, live broadcast ended |
| **Chat write** | Send a chat message to your active broadcast |
| **Quota guard** | Automatic long-interval fallback when daily API quota approaches the limit |

---

## Connecting your YouTube channel

1. Open **Settings → Platforms → YouTube** and click **Connect**.
2. forge displays a short user code and a verification URL.
3. On any browser (phone works too), open **https://google.com/device**, enter the code, and
   choose the YouTube channel you want forge to use.
4. Grant the permissions listed on the consent screen, then return to forge. The UI confirms
   connection and shows your channel name.

forge stores the refresh token in your local encrypted credentials database. Access tokens are
renewed automatically roughly 5 minutes before they expire — you will not be asked to
re-authenticate on every launch.

### Unverified app warning

During the OAuth consent screen, Google may show an **"unverified app"** warning. This is
expected. It is a procedural step in Google's OAuth verification process, not a security
finding — no data leaves your machine beyond what the YouTube API requires.

To proceed: click **Advanced** → **Go to forge (unsafe)**.

**Why this warning appears:** Google requires app owners to submit a formal verification
request before the consent screen appears without the warning. This verification process
can take 3 to 6 weeks. While it is in progress, Google also caps the total number of
distinct accounts that can authorize the app at **100**. forge is working through this
process; the cap will be lifted once verification completes.

If you are an early tester and encounter a "This app is blocked" error (not just the
warning), the 100-user cap has been reached. Check the forge release notes for an updated
timeline.

---

## What forge listens to

forge converts YouTube live chat events into triggers that can fire any action chain you
configure. The table below lists every trigger kind and the variables available in
`%variable%` interpolation within sub-actions.

### Chat triggers

| Trigger kind ID | Fires when | Available variables |
| :--- | :--- | :--- |
| `youtube.chat.message` | Any non-command chat message arrives | `message_text`, `user_display_name`, `channel_id` |
| `youtube.chat.command` | Message starts with `!` | `message_text`, `command_name`, `args`, `user_display_name`, `channel_id` |

`command_name` is the word immediately after `!` (without the `!`). `args` is everything
after the command name, trimmed.

### Monetization triggers

| Trigger kind ID | Fires when | Available variables |
| :--- | :--- | :--- |
| `youtube.support.super_chat` | Viewer sends a Super Chat | `user_display_name`, `amount_micros`, `currency`, `message_text` |
| `youtube.support.super_sticker` | Viewer sends a Super Sticker | `user_display_name`, `sticker_id`, `amount_micros`, `currency` |
| `youtube.support.new_member` | Viewer becomes a new channel member | `user_display_name`, `member_level_name` |
| `youtube.support.member_milestone` | Member sends a milestone message | `user_display_name`, `member_month`, `message_text` |

`amount_micros` is the monetary value in millionths of the currency unit. For example, a
$5.00 Super Chat has `amount_micros = 5000000` and `currency = USD`. Convert to a
whole-unit value in a Rhai script with `amount_micros / 1_000_000.0`.

### Moderation triggers

| Trigger kind ID | Fires when | Available variables |
| :--- | :--- | :--- |
| `youtube.moderation.timeout` | Viewer is temporarily banned from chat | `user_display_name`, `ban_duration_seconds` |
| `youtube.moderation.ban` | Viewer is permanently banned from chat | `user_display_name` |

### Broadcast lifecycle triggers

| Trigger kind ID | Fires when | Available variables |
| :--- | :--- | :--- |
| `youtube.channel.live_broadcast_started` | forge detects a new active broadcast | `broadcast_title`, `broadcast_id` |
| `youtube.channel.live_broadcast_ended` | forge detects the broadcast has ended | `broadcast_id` |

Broadcast detection is based on polling `liveBroadcasts.list` every 60 seconds. There is
an inherent delay of up to 60 seconds between when you go live and when forge fires
`live_broadcast_started`.

---

## What forge can do to YouTube

### `youtube.send_chat` sub-action

Sends a message to your currently-active live broadcast's chat.

**Requirements:**
- forge must be connected to YouTube.
- You must have an active live broadcast running. forge cannot send to a broadcast that
  has not started or has already ended.

**Quota cost:** each send costs 50 API units (see [Polling and quotas](#polling-and-quotas)).

---

## Polling and quotas

YouTube's Data API v3 allocates **10,000 units per day** per Google Cloud project. forge
uses a shared project for all users, so quota is pooled and managed carefully.

### How forge uses quota

| Operation | Units per call | Cadence |
| :--- | ---: | :--- |
| Detect active broadcast (`liveBroadcasts.list`) | 1 | Every 60 s |
| Poll live chat messages | 5 | Per `pollingIntervalMillis` (min 3 s) |
| Send a chat message | 50 | On each `youtube.send_chat` execution |
| Moderation actions (ban, mod) | 50 | On each moderation sub-action |

### Polling interval

forge honors the `pollingIntervalMillis` value returned by YouTube in every chat-poll
response. YouTube dynamically increases this value for lower-traffic streams to reduce
unnecessary API calls. The local minimum floor is **3 seconds** — forge will never poll
faster than that, regardless of what the API returns.

### Quota high-water guard

When forge has used **9,000 of 10,000 daily units**, it automatically switches to
**long-interval mode**: the chat poll interval is forced to a minimum of 60 seconds,
regardless of what YouTube returns. A warning appears in the UI.

Chat polling continues in long-interval mode — you will not miss events entirely, but
there will be up to a 60-second delay before they fire.

Quota resets at **midnight Pacific Time** each day (07:00 or 08:00 UTC depending on DST).
forge resets its local counter at the same time.

### Quota display

The YouTube detail screen in **Settings → Platforms → YouTube** shows:

- Units used today
- Lifetime peak usage
- Time until daily reset
- Whether long-interval mode is active

---

## Membership events — current limitations

forge surfaces membership events that appear in the live chat feed:

- **New member** (`youtube.support.new_member`) — fires when a viewer joins via
  `newSponsorEvent` in the chat feed.
- **Milestone message** (`youtube.support.member_milestone`) — fires when an existing
  member sends a milestone chat message.

**What is NOT surfaced in the current release:**

- **Tier upgrades and cancellations** that do not produce a chat event.
- **"Is this user currently a member?"** lookups — querying a viewer's current
  membership status requires a partner-only API that forge does not yet have access to.

These gaps will be addressed in a future release when the required API access becomes
available. Existing `youtube.support.*` triggers you configure today will continue to work
when the broader membership data becomes available.

---

## Disconnecting

**Settings → Platforms → YouTube → Disconnect**

This deletes the stored access and refresh tokens. forge will no longer poll YouTube until
you reconnect.

Your existing triggers that reference `youtube.*` trigger kinds are retained. They will
remain dormant until you reconnect. No trigger configuration is lost on disconnect.

---

## Troubleshooting

### "Unverified app" warning on the consent screen

Expected behavior — see [Connecting your YouTube channel](#connecting-your-youtube-channel).
Click **Advanced → Go to forge (unsafe)** to proceed.

### "Daily quota exhausted" warning in the UI

forge has switched to long-interval mode (60-second polling). Chat events are still
captured, with up to a 60-second delay. Normal polling resumes automatically at midnight
Pacific Time. Reducing the frequency of `youtube.send_chat` sub-actions conserves quota.

### "No active broadcast" error when trying to send a chat message

The `youtube.send_chat` sub-action requires a live broadcast in progress. Start streaming
on YouTube before triggering this sub-action.

### Token refresh failed — forge is asking me to reconnect

Go to **Settings → Platforms → YouTube → Reconnect** and complete the device-code flow
again. This replaces the expired or revoked token. Your trigger and action configuration
is not affected.

### Broadcast lifecycle triggers are not firing

forge checks for an active broadcast every 60 seconds. If you went live recently, wait
up to 60 seconds. If you have been live for several minutes and the trigger has not fired,
verify that forge is connected (Settings → Platforms → YouTube shows "Connected") and that
your broadcast is visible to the authorized channel.

### Chat messages appear delayed

YouTube controls the polling interval via `pollingIntervalMillis`. On quiet streams,
YouTube may return an interval of 15–30 seconds or more, and forge honors that. If
long-interval mode is active due to quota, the delay extends to up to 60 seconds.

### Connection drops mid-stream

forge will attempt to reconnect automatically using the stored refresh token. If the
token has expired or been revoked, forge surfaces a re-auth prompt via
**Settings → Platforms → YouTube → Reconnect**.
