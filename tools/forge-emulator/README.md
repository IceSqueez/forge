# forge-emulator

A black-box integration harness for forge. It seeds a throwaway forge installation, stands
up fake platform services on loopback, launches the real forge binary against them, drives
it through a scenario written as JSON, and observes the result over forge's own control
socket (`/ws/v1/`) plus its log file. Nothing about forge is mocked: the binary under test
is the one a user would run.

This is a nested workspace with its own `Cargo.lock`. The repository gate never builds it;
`./check.sh` is its gate (build, fmt, clippy, doc, test), and it must pass before any
change here is committed.

## Build

```sh
env -u RUSTUP_TOOLCHAIN cargo build            # from the repository root: the forge binary
cd tools/forge-emulator && cargo build         # the emulator itself
```

## Run a scenario

```sh
tools/forge-emulator/target/debug/forge-emulator scenario run \
  tools/forge-emulator/scenarios/chat-command-single-viewer.json \
  --forge target/debug/forge \
  --run-root /path/to/scratch/run
```

`--run-root` is the parent of the per-attempt directories; a temp directory is used when it
is omitted. Useful flags: `--log` sets forge's `RUST_LOG` filter (only the targets it
enables are visible to `log_line` expectations), `--attempts` caps the relaunches allowed
when forge loses the race for its seeded server port, `--allow-over-game` starts forge even
while a game is on screen (see below).

Other subcommands: `scenario check <file>` validates a scenario without launching anything,
`launch` boots a seeded forge and streams its events as JSON, `watch` attaches to an already
running forge, and `seed` fills an empty data directory from a fixture on stdin.

## Scenario steps

| Step | What it does |
| :--- | :--- |
| `forge_ready` | waits until forge authenticates a control connection; always the first step |
| `twitch_subscribed` | waits until the fake Twitch holds a subscription of every listed type |
| `chat` | one viewer sends one chat message |
| `crowd` | a crowd of viewers chats, a share of them sending commands |
| `twitch_event` | delivers one EventSub notification of any type (see below) |
| `session_reconnect` | Twitch asks forge to move to a successor EventSub session |
| `overlay_page` | opens a browser source for a fixture overlay |
| `pause` | a fixed wait, with a mandatory reason |
| `run_action` | runs a fixture action by name, as a dashboard would |
| `set_global` | sets a global over the control socket |
| `obs_online` | starts a fake OBS configured with `online_at_boot: false` |
| `obs_identified` | waits until forge holds an identified OBS session |
| `obs_restart` | OBS quits (every session closed, port refused for `down_ms`) and starts again |
| `obs_scene_switch` | the streamer switches the OBS program scene |
| `obs_stream` | the streamer starts or stops streaming in OBS |
| `obs_input_mute` | the streamer mutes or unmutes an OBS input |
| `vtube_online` | starts a fake VTube Studio configured with `online_at_boot: false` |
| `vtube_authenticated` | waits until forge holds an authenticated VTube Studio session |
| `vtube_hotkey` | the streamer presses a hotkey of the loaded model |
| `vtube_model_load` / `vtube_model_unload` | the streamer loads another model or unloads the current one |
| `vtube_model_config_changed` | the streamer changes the loaded model's settings |
| `vtube_tracking` | the tracker finds or loses the streamer's face |
| `vtube_item_added` / `vtube_item_removed` | the streamer drops an item into the scene or removes it |
| `vtube_expression` | the streamer turns an expression on or off; forge sees it on its next expression poll |
| `donatello_donation` | a viewer donates on the fake Donatello |
| `monobank_top_up` | a viewer tops up the seeded jar on the fake monobank |
| `donations_polled` | waits until every fake donation service serves a donation list requested after the step began |
| `forge_restart` | quits forge, hands the `offline` donations to the fakes while it is down, starts it again on the same data and waits until it is ready |

## Donation services

`fixture.donatello: {}` seeds the fake Donatello token and `fixture.monobank: { "jar_id": "..." }`
seeds the fake monobank token with the jar forge watches; each needs its `fakes.donatello` /
`fakes.monobank` entry, whose `history` lists donations the service already holds at launch.
A gift is `{ "id", "donor"?, "amount", "message"?, "minutes_ago"? }` on Donatello and
`{ "id", "sender", "amount_minor", "comment"?, "minutes_ago"? }` on monobank; `minutes_ago` dates
it before the moment it is handed to the fake, and Donatello wall-clock times are written in the
time zone forge reads them in. `forge_restart` lists its gifts as
`{ "donatello": {...} }` / `{ "monobank": {...} }`. The journal keeps every event across the
restart, so expectations on the restart step see only what the new process published.

`fake-donatello [--token T] [--nickname N] [--profile-incomplete]` and
`fake-monobank [--token T] [--jar ID=TITLE]...` run one fake alone for a forge started elsewhere.
Each prints the endpoint override, its token (and, for monobank, every jar id), one `KEY=VALUE` per
line, then reads commands from stdin and answers each with an `ok ...` line:

```text
donate <donor> <amount> [message...]        # Donatello: a new donation, dated now
fail 429 [retry_after_secs] | fail <status> | fail malformed   # Donatello: next list fails
profile complete | profile incomplete       # Donatello: token checks see the profile state
top-up <jar> <amount_minor> <sender> [comment...]   # monobank: a top-up into a served jar
```

A refused command is reported on stderr and the fake keeps serving until interrupted.

## Triggers other than chat commands

A fixture declares chat commands and event triggers side by side; both seed an action, a
trigger instance and the binding between them, and both rewrite `overlay.send` targets from
an overlay's display name to its minted identity.

```json
"event_triggers": [
  { "trigger_kind": "twitch.support.subscriber", "action_name": "Announce Subscriber",
    "config": { "tier": { "type": "string", "value": "1000" } },
    "steps": [...] }
]
```

`trigger_kind` is a trigger **descriptor** id, the string a trigger registry answers to, not
the event kind that fires it: the new-subscriber trigger is `twitch.support.subscriber` and
listens for the event `twitch.channel.subscribe`. An action name is unique across both lists,
because `run_action` resolves an action by that name alone.

The step that fires such a trigger is

```json
{ "do": { "twitch_event": { "subscription_type": "channel.subscribe",
                            "event": { "user_login": "luckyviewer", "tier": "1000" } } } }
```

`event` is the object Twitch would put under `payload.event`, copied from the EventSub
reference for that subscription type; the harness never invents its shape. The frame reaches
every live session subscribed to `subscription_type`, so a `twitch_subscribed` step for that
type belongs before it - a scenario that skips one is rejected before launch, and a type no
live session holds fails the step with the list of the ones that are held.
`scenarios/subscription-raises-an-alert.json` runs this path end to end.

## Overlays

A fixture can declare overlays beside its chat commands:

```json
"overlays": [
  { "display_name": "Alert Box", "kind_id": "overlay.alert",
    "config": { "headline": { "type": "string", "value": "stored headline" } } }
]
```

The seeder creates each one through forge's own overlay repository, which mints the identity
slug and the page credential; the fixture never picks either. A scenario names an overlay by
its `display_name` everywhere, and an `overlay.send` step whose `overlay_id` holds a declared
display name is rewritten to the minted identity before it reaches storage. A display name no
overlay declares, or an overlay type this build does not carry, fails the seed rather than
producing a step that addresses nothing. Page credentials are scrubbed from both report files.

Two pieces of scenario vocabulary follow:

- the step `{ "overlay_page": { "overlay": "Alert Box", "within_ms": 30000 } }` opens a browser
  source for that overlay: it fetches `/overlays/<identity>/config.json` over HTTP, opens
  `ws://127.0.0.1:<port>/ws/v1/` with the matching `Origin`, and presents the credential the
  document carries, exactly as `crates/forge-overlay/assets/shared/runtime-v1.js` does. The step
  succeeds only once forge has accepted the credential.
- the expectation `{ "overlay_content": { "overlay": "Alert Box", "values": { "headline":
  "alice raised an alert" }, "within_ms": 5000 } }` holds when that page receives a content
  frame carrying every named key as exactly that plain JSON string. Expected values are always
  literals: the harness never re-derives what forge should have interpolated. A frame whose
  content holds a `{"type": ..., "value": ...}` object at any depth fails the expectation even
  when the named keys read correctly - that is the tagged `Variant` shape a browser renders as
  `[object Object]` - and the report names the pointers and prints the raw frame.

Content delivered while the credential is still being checked counts as the opening step's own,
so a page opened after a Replace-kind delivery can be judged on the content forge replays to it.

## Fixture queues

A fixture can create queues beside the built-in `Default` one, and any chat command or event
trigger can put its action on one by name:

```json
"queues": [{ "name": "Alerts", "concurrency": 1 }, { "name": "Chat", "concurrency": 4 }],
"event_triggers": [{ "trigger_kind": "twitch.channel.follow", "action_name": "Follow Alert",
                     "queue": "Alerts", "steps": [...] }]
```

Concurrency 1 is a blocking queue. An action with no `queue` runs on `Default`; naming a queue
the fixture does not declare fails validation.

## Discord webhooks

`fixture.discord_webhooks` names the webhooks a step may post to, and `fakes.discord: {}`
starts a fake Discord that answers them. The run seeds each webhook's credential with the
fake's loopback address, so forge posts there with no endpoint override: forge checks the
`discord.com` host only when a user saves a webhook, never when it loads a stored one.

```json
"discord_webhooks": [{ "name": "go-live" }]
```

The fake answers like Discord: `200` with the created message when the post carries
`?wait=true`, `204` without it, `200` to an edit and `204` to a delete under
`/messages/{id}`, and `404` with code `10015` to an id or token it does not know. Every
request is recorded with its content, embeds, `allowed_mentions` and uploaded file names.
The `discord_post` expectation passes when, after the step acts, the webhook accepted a post
matching every given condition:

```json
{ "discord_post": { "webhook": "go-live", "content_contains": "<@&123>",
                    "mention_parse": ["users", "roles"], "within_ms": 5000 } }
```

`mention_parse` is compared as a set against `allowed_mentions.parse`; a post that carries no
`allowed_mentions` never matches it.

## OBS

`fixture.obs` seeds forge's OBS connection and `fakes.obs` starts a fake OBS Studio that speaks
obs-websocket v5 ([protocol](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md)).
The run fills the seeded connection's port with the fake's loopback port, so forge dials the
fake through its ordinary stored credential and needs no endpoint override.

```json
"fixture": { "obs": { "password": "secret" } },
"fakes": { "obs": { "password": "secret", "scenes": ["Main", "BRB"], "current_scene": "Main",
                    "inputs": [{ "name": "Mic/Aux", "kind": "pulse_input_capture" }],
                    "online_at_boot": true } }
```

Every `fakes.obs` field is optional; the defaults are no password, scenes `Main` and `BRB`, one
`Mic/Aux` input and an OBS that is already running when forge starts. With a password the
fake sends an authentication challenge in `Hello` and closes a wrong answer with
`AuthenticationFailed` (4009). It answers the requests forge's OBS client sends - `GetVersion`,
`GetStats`, the scene, scene-item and input getters, `SetCurrentProgramScene`, `SetInputMute`,
`ToggleInputMute`, `GetStreamStatus`, `StartStream`, `StopStream` and `GetRecordStatus` - and
`UnknownRequestType` (204) to anything else. A change made by a request or a step is pushed as
the spec's event (`CurrentProgramSceneChanged`, `StreamStateChanged`,
`InputMuteStateChanged`) to every identified session subscribed to its category. Request
batches are not modelled.

Two expectations read the fake's ledger, counting only what happened after the step started:

```json
{ "obs_request": { "request_type": "SetCurrentProgramScene",
                   "request_data": { "/sceneName": { "equals": "BRB" } }, "code": 100,
                   "within_ms": 5000 } }
{ "obs_auth": { "accepted": false, "within_ms": 45000 } }
```

forge connects to OBS during boot, before the harness subscribes to its events; a scenario that
judges forge's connection events starts the fake with `online_at_boot: false` and an
`obs_online` step, so forge's retry loop connects while the harness is listening.
`fake-obs [--password P] [--scene NAME]...` runs the fake alone and prints its address.

## VTube Studio

`fixture.vtube` seeds forge's VTube Studio plugin token and `fakes.vtube` starts a fake VTube
Studio that speaks the public API 1.0 ([API](https://github.com/DenchiSoft/VTubeStudio),
[events](https://github.com/DenchiSoft/VTubeStudio/tree/master/Events)). As with OBS, the run
fills the seeded connection's port with the fake's loopback port, so forge dials the fake
through its ordinary stored credential and needs no endpoint override.

```json
"fixture": { "vtube": { "token": "emulator-vtube-token" } },
"fakes": { "vtube": { "token": "emulator-vtube-token", "approve_token_requests": true,
                      "models": [{ "name": "Emulator Avatar", "hotkeys": ["Wave", "Blush"],
                                   "expressions": ["Blush.exp3.json", "Smile.exp3.json"] }],
                      "current_model": "Emulator Avatar", "items": ["emulator_star.png"],
                      "parameters": ["FaceAngleX", "MouthOpen"], "face_found": true,
                      "online_at_boot": true } }
```

Every field is optional and the example shows the defaults (the default `parameters` also list
`FaceAngleY` and `MouthSmile`; `current_model` defaults to the first model). An
`AuthenticationRequest` with any other token is answered `authenticated: false`, which forge
treats as a revoked token. Until a session authenticates, everything except `APIStateRequest`
and the two authentication requests is refused with `RequestRequiresAuthetication` (8). The
fake answers the requests forge's VTube Studio client sends - the model, hotkey, expression,
parameter, item, tint and physics requests behind its sub-actions - with the documented payloads
and `ErrorID.cs` errors, and `RequestTypeUnknown` (7) to anything else. A change made by a request
or a step is pushed as the documented event (`ModelLoadedEvent`, `ModelConfigChangedEvent`,
`HotkeyTriggeredEvent`, `TrackingStatusChangedEvent`, `ItemEvent`, `ExpressionToggledEvent`) to
every authenticated session subscribed to it; a hotkey forge triggers comes back with
`hotkeyTriggeredByAPI: true`. Permissions, post-processing and custom parameters are not
modelled.

Two expectations read the fake's ledger, counting only what happened after the step started.
`succeeded` asks for an answer without an error and `error_id` for that `ErrorID.cs` error:

```json
{ "vtube_request": { "message_type": "HotkeyTriggerRequest",
                     "data": { "/hotkeyID": { "equals": "Wave" } }, "succeeded": true,
                     "within_ms": 5000 } }
{ "vtube_auth": { "accepted": false, "within_ms": 45000 } }
```

forge connects during boot, so a scenario that judges forge's connection events starts the fake
with `online_at_boot: false` and a `vtube_online` step. forge publishes VTube Studio events with
the source `v_tube`. `fake-vtube [--token T] [--deny-token-requests]` runs the fake alone and
prints its address.

## Kick

`fixture.kick` seeds forge's Kick credentials and client id and secret, and `fakes.kick` starts
a fake Kick: one loopback HTTP server for the official public API (`/public/v1`), the channel
API forge reads the chatroom id from (`/api/v2`) and the OAuth token endpoint (`/oauth`), plus
a Pusher WebSocket (`/app/<key>`) for chat receive. The run points forge at it through
`FORGE_KICK_API_BASE_URL`, `FORGE_KICK_CHANNEL_API_BASE_URL`, `FORGE_KICK_OAUTH_BASE_URL` and
`FORGE_KICK_CHAT_WS_BASE_URL`, and passes the fixture's client id and secret as
`FORGE_KICK_CLIENT_ID` / `FORGE_KICK_CLIENT_SECRET`. Browser login is not modelled: the
credentials are seeded, never obtained.

```json
"fixture": { "kick": { "username": "forge_emulator", "user_id": 200000001,
                       "token_expired": false } },
"fakes": { "kick": { "chatroom_id": 300000001, "live_at_boot": false,
                     "stream_title": "forge emulator stream", "category_id": 15,
                     "category_name": "Just Chatting", "viewer_count": 42,
                     "refresh": "accept" } }
```

Every field is optional and the example shows the defaults; the fixture also takes
`client_id`, `client_secret`, `access_token` and `refresh_token`. `token_expired` seeds an
expiry an hour in the past, so forge refreshes before its first authorized call. The public API
accepts only the current access token (a refresh rotates both tokens and retires the old ones),
and `"refresh": "reject"` answers every refresh with 400 `{"error": "Invalid request"}`, as Kick
documents. A wrong app key on the socket gets `pusher:error` 4001. The fake models chat send and
delete, the channel read, the pending-redemption list, the channel lookup and the token refresh;
any other request is answered 404 and counted as unexpected.

Steps: `kick_chat_joined` waits for a connection subscribed to `chatrooms.<id>.v2`; `kick_chat`
pushes a `ChatMessageEvent` from `sender` (`user_id`, `username`); `kick_pusher_event` pushes any
event name with a JSON object as its data; `kick_stream` flips the channel live or offline for
forge's next poll; `kick_channel_polled` waits for a channel read after the step starts (forge
polls every 30 seconds). Expectations: `kick_request` (method, path, body pointers, status,
within the step), `kick_request_count` and `kick_no_unexpected_requests` (whole run). forge
publishes Kick events with the source `kick`; chat commands are `event_triggers` with the
`kick.chat.command` descriptor.

## YouTube

`fixture.youtube` seeds forge's YouTube credentials and `fakes.youtube` starts a fake YouTube: one
loopback HTTP server for the Data API (`/youtube/v3`), the upload API (`/upload/youtube/v3`) and
the Google token endpoint (`/oauth2/token`). The run points forge at it through
`FORGE_YOUTUBE_API_BASE_URL`, `FORGE_YOUTUBE_UPLOAD_BASE_URL` and `FORGE_YOUTUBE_OAUTH_BASE_URL`.
Browser login is not modelled: the credentials are seeded, never obtained.

forge wires YouTube only when it was **built** with non-empty `FORGE_YOUTUBE_CLIENT_ID` and
`FORGE_YOUTUBE_CLIENT_SECRET` (they are read at compile time, so the launch cannot supply them).
Any values work against the fake, which accepts every client id; a build without them never
polls, and `youtube_chat_polled` fails saying so. To build a binary for these runs without
touching your own one:

```sh
FORGE_YOUTUBE_CLIENT_ID=emulator FORGE_YOUTUBE_CLIENT_SECRET=emulator \
  CARGO_TARGET_DIR=/path/to/scratch/yt-target env -u RUSTUP_TOOLCHAIN cargo build
```

```json
"fixture": { "youtube": { "channel_id": "UCemulatorForgeChannel01", "channel_title": "forge emulator",
                          "channel_handle": "@forge_emulator", "token_expired": false } },
"fakes": { "youtube": { "live_at_boot": false, "broadcast_id": "EmuBroadcst",
                        "live_chat_id": "EmulatorLiveChatId0001",
                        "broadcast_title": "forge emulator stream", "concurrent_viewers": 42,
                        "polling_interval_ms": 3000, "refresh": "accept" } }
```

Every field is optional and the example shows the defaults; the fixture also takes
`access_token` and `refresh_token`. `token_expired` seeds an expiry an hour in the past, so forge
refreshes before its first authorized call. The Data API accepts only the current access token
and answers anything else with Google's 401 `authError`; a refresh rotates the access token only,
as Google does, and `"refresh": "reject"` answers it with 400 `invalid_grant`. Shapes follow the
[Live Streaming API reference](https://developers.google.com/youtube/v3/live/docs): the active
broadcast list (`liveBroadcasts?broadcastStatus=active`) carries the `liveChatId`; the chat list
returns every message added since the `pageToken` it issued, a `nextPageToken` and
`pollingIntervalMillis`; ending the broadcast adds a `chatEndedEvent` and `offlineAt`, after which
the chat list and inserts answer 403 `liveChatEnded`. A `textMessageEvent` insert is echoed back
into the chat as the channel owner, as YouTube does. The fake also models `videos` (viewer count,
title) and `channels` for the seeded channel; any other request is answered 404 and counted as
unexpected.

forge resolves the broadcast once a minute, so a broadcast started after boot can take up to a
minute to reach it; it learns the end from the chat poll. Steps: `youtube_chat_polled` waits for a
successful chat poll after the step starts; `youtube_chat` adds a `textMessageEvent` from `author`
(`channel_id`, `display_name`, optional `sponsor`, `moderator`); `youtube_chat_event` adds any
message with the given `snippet` (copied from the liveChatMessage reference, e.g. a
`superChatEvent` with `superChatDetails`); `youtube_broadcast` starts or ends the broadcast. Chat
steps need the broadcast live at that point. Expectations: `youtube_request` (method, path, body
pointers, status, within the step), `youtube_request_count` and `youtube_no_unexpected_requests`
(whole run). The recorded refresh form never keeps `client_secret`. forge publishes YouTube events
with the source `you_tube`.

## Throughput runs

`stress run <profile> --forge <binary>` is a separate mode from scenarios: it seeds the profile's
fixture, connects a browser source to every overlay in `pages`, sets the profile's `globals`
over the control socket, and then drives a load instead of checking expectations. Profiles live
in `stress/`; `throughput-smoke.json` proves the tooling in about a minute and
`throughput-ramp.json` is the full run; `throughput-fine.json` re-steps the region around the
knee one minute per step. `stress check <profile>` validates one without launching.

The load has two parts. Stimuli with a `weight` share the flood rate, which steps through
`ramp.rates` (events per second), each step held for `ramp.hold_secs`. Stimuli with a
`per_minute` trickle in at that rate whatever the step, the way alerts do on a real stream, so a
blocking alert queue is never flooded by design. Senders cycle through `senders` distinct
viewers. After each step the `knee` rule judges it: the step is degraded when the generator
delivered less than `min_achieved_share` of the target because forge stopped reading its
EventSub socket, when forge logged any message listed in `fatal_warnings` (the throughput
profiles list the trigger evaluator's "those events fired no trigger", which means actions were
lost), or when the watched actions' unfinished work exceeds `max_backlog_secs` of the step's rate
or any of them was skipped. The last check is skipped for a step in which forge's server dropped
events for the observer, because its counts undercount then; the phase table shows how many. The ramp stops at the first degraded step, forge is left to
recover for `recovery_secs`, a burst runs at `burst.multiplier` times the last stable step for
`burst.secs`, and a second recovery follows.

Every injected event carries a marker (`~q<sequence>~`) in its chat text or its `user_name`, and
the actions render those variables into their overlay sends and chat replies, so each effect is
traced back to the moment it was injected. Per phase the report gives p50/p95/p99 latency from
injection to the Twitch event forge published, to `action.start` and `action.done` per action,
to the frame each connected page received, and to the chat reply reaching the fake Helix;
per-action started/done/skipped/unstarted/in-flight counts at each phase end; process CPU (whole
process and the UI thread), RSS and anonymous memory from `smaps_rollup`, threads, file
descriptors, database size, table row counts, the machine's load average, and every WARN/ERROR
log line grouped by message. `samples.jsonl` holds one row per `sample_ms`.

Measure memory only on a release forge build, and run with the default `--log info`: a debug
filter makes logging the bottleneck. The observer subscribes to every Twitch event and to
`action.start`, `action.done`, `action.skipped`, `command.matched` and `chat.send.failed`, so it
costs forge what one connected dashboard would.

## Where reports land

Each run writes `report.md` and `report.json` into the run root, and prints the verdict line
followed by the path to the Markdown. The Markdown is a ready-to-file bug report: run
metadata (including the forge version it interrogated over the control socket), a repro
command, a per-step table, one titled bug per failed action or expectation with its matcher
and evidence, and the tail of forge's stdout, stderr, and log file. Every run's own secrets
(the seeded bearer token, the fixture access token) are scrubbed out of both files.

Per-attempt state lives under `<run-root>/attempt-N/`: the forge data directory, its
database, and its log directory. It survives the run, so a failure can be dug into
afterwards.

## Exit codes

| Code | Meaning |
| ---: | :--- |
| 0 | every step passed |
| 1 | harness failure, or the report could not be written |
| 3 | refused: a game may be running |
| 4 | refused: the run would touch the live data directory or home |
| 5 | forge exited before the harness asked it to |
| 6 | forge refused an endpoint override |
| 7 | forge lost the race for its seeded server port on every attempt |
| 8 | forge never became ready |
| 9 | the scenario file could not be read |
| 10 | the scenario file is not valid JSON for a scenario |
| 11 | the scenario parses but is semantically invalid |
| 12 | the scenario ran and failed |
| 130 | interrupted |

## The game guard

`scenario run` and `launch` open a real forge window. Both ask the compositor what is on
screen and refuse to start while a fullscreen Steam or gamescope window is up. The guard
fails closed: an unreadable answer, a missing compositor, or a slow one all refuse too, and
every refusal exits 3. The guard is a required argument of the process spawner, so no code
path can skip it by accident. If it refuses, stop: do not retry and do not work around it.

The one way past it is `--allow-over-game` on `launch` or `scenario run`, which skips the
compositor query entirely and prints that it did. It is a flag and nothing else: no
environment variable can disable the guard, so a stale shell can never silence it. Without
the flag the guard still runs and still fails closed.

The launcher separately refuses to run against the maintainer's live data directory or home
(exit 4). Every run gets a scratch `HOME`, scratch XDG directories, and an environment
built from an allowlist rather than inherited.

## Scenarios

An event expectation names the `kind` string exactly as forge publishes it; the wiki page
`reference/event-kinds.md` is the canonical roster. Getting it wrong is not a loud failure:
the harness subscribes only to the kinds a scenario names, so an unknown kind produces
silence that reads like forge never emitted the event. Check the kind against the wiki
before blaming forge for a missing event. A Twitch chat message, for instance, is
`twitch.channel.chat.message`, not `chat.message`. Payload pointers deserve the same care:
`action.start` carries `action_name`, `action.done` carries only the `action_id` it shares
with its cause, so an outcome is pinned to an action through a `caused_by` and not through a
name matcher on `action.done`.

`scenarios/` holds the scenario set, and every one of them is expected to pass. Two of them
cover the overlay chain end to end: `chat-command-raises-an-alert.json` runs a chat command
whose step sends to an alert overlay and judges the frame a connected page receives, and
`overlay-page-replays-retained-content.json` opens a page only after a goal overlay was
already updated, so the retained content is what it has to render. Both are written so that
content arriving as tagged `Variant` JSON fails them.

`subscription-raises-an-alert.json` covers the same overlay chain from a trigger that is not a
chat command: a `channel.subscribe` notification runs the action bound to the new-subscriber
trigger instance, and the page is judged on the headline it receives.

Four more follow that shape, one per Twitch event a live channel produces without a chat
command, so a regression in any of them is caught off-stream:

| Scenario | Notification | Trigger descriptor | Headline |
| :--- | :--- | :--- | :--- |
| `follow-raises-an-alert.json` | `channel.follow` | `twitch.channel.follow` | `%user_name%` |
| `raid-raises-an-alert.json` | `channel.raid` | `twitch.channel.raid_received` | `%user_name%`, `%viewer_count%` |
| `cheer-raises-an-alert.json` | `channel.cheer` | `twitch.support.cheer` | `%user_name%`, `%bits_amount%` |
| `reward-redemption-raises-an-alert.json` | `channel.channel_points_custom_reward_redemption.add` | `twitch.channel_points.redemption` | `%user_name%`, `%reward.title%` |

Each injects the payload the EventSub reference documents for that subscription type, pins two
or three pointers into the bus event forge publishes from it, and judges the connected page on
a headline whose every token is a canonical variable. In each one a display name that is not
the login, and a reward title that is not the reward id, keep a page that renders the wrong
field from passing. Two carry a trigger condition the injected event has to satisfy: the cheer
instance sets `min_bits`, and the redemption instance sets `reward_id` - both plain text fields
on the trigger, so no reward has to exist in storage for the fixture to be complete. The raid
is delivered as received rather than sent only because its `to_broadcaster_user_id` is the
seeded account's, which is what forge reads to tell the two directions apart on the one topic.

`crowd-soak-short.json` is the load case rather than a feature case: a thousand viewers send
nine chatter lines and one command each, ten thousand messages twelve milliseconds apart, so the
load is sustained for about two minutes instead of bursting. It asserts one command match and one
finished action per sender, no `request.fail` for the whole run, and no unexpected request to the
fake. A crowd may hold up to 100 000 viewers and a million messages.

Four scenarios cover Kick: `kick-chat-command-replies-in-chat.json` (chat command, reply through
the send endpoint), `kick-gifted-subs-thank-the-gifter.json` (`GiftedSubscriptionsEvent` to the
gift trigger and its variables), `kick-stream-online-and-offline.json` (channel poll to the
livestream status trigger, both directions) and `kick-refresh-failure-is-visible.json` (a refused
refresh fails the send step with a re-authentication message and posts nothing).

`reconnect-keeps-subscriptions.json` was the first defect this harness found - forge ran a
second full subscription pass on a successor EventSub session - and was kept red as evidence
until that was fixed. It has passed since; if it ever fails again, the regression is forge's.

`creator-goals-sync-on-connect.json` covers state forge reads on connect rather than an
event it is sent. `fakes.twitch.goals` lists the creator goals the fake serves on
`GET /helix/goals` (`{ "id", "type", "description"?, "current_amount", "target_amount" }`; empty
by default, a missing `broadcaster_id` answers 400 and another broadcaster's id 401). The goal
fires the goal-progress trigger before any observer is attached, so the scenario binds it to a
goal overlay and opens the page afterwards: the retained content, flagged `synced true`, is the
proof. A `session_reconnect` then must not request the goals a second time.

`GET /helix/channels/followers` answers from the `channel.follow` events the run injected: each
one records its `user_id`, `user_login`, `user_name` and `followed_at`, and a repeat follow by the
same user replaces the earlier row. `user_id` narrows the answer to that viewer and `first` caps
the rows (newest first). A missing `broadcaster_id` answers 400; another broadcaster's id answers
200 with no rows, as Twitch does for a token without moderator access to that channel.

`go-live-pings-the-discord-role.json` follows a `stream.online` notification to a Discord
webhook post and judges the post's `allowed_mentions`: the role ping is parsed and
`@everyone` is not.

Five scenarios cover OBS: `obs-connects-and-reads-scenes.json` (authenticate, identify, load
the scene list), `obs-action-switches-scene.json` (a Switch Scene step sends
`SetCurrentProgramScene` and the change comes back as `obs.scene.changed`),
`obs-events-fire-triggers.json` (scene, stream and mute changes made in OBS reach forge's
triggers), `obs-restart-reconnects.json` (forge reconnects on its own after OBS restarts) and
`obs-wrong-password-fails-visibly.json` (a wrong password publishes
`obs.connection.auth_failed` and is not retried).

Three scenarios cover VTube Studio: `vtube-action-triggers-hotkey.json` (a Trigger Hotkey step
sends `HotkeyTriggerRequest` and the hotkey comes back as `vtube.hotkey.triggered`),
`vtube-events-fire-triggers.json` (hotkey, tracking, item and expression changes made in VTube
Studio reach forge's triggers) and `vtube-revoked-token-fails-visibly.json` (a revoked token
publishes `vtube.connection.changed` with reason `auth_required` and is not retried).
