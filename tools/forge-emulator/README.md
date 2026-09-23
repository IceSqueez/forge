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

`reconnect-keeps-subscriptions.json` was the first defect this harness found - forge ran a
second full subscription pass on a successor EventSub session - and was kept red as evidence
until that was fixed. It has passed since; if it ever fails again, the regression is forge's.
