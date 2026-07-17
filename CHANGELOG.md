# Changelog
All notable changes to this project will be documented in this file.

## [0.3.0-beta.4] - 2026-07-17
### ⚡ Performance
- *(chat)* Memoize drawer summaries and virtualize the message list

### 🎨 Styling
- *(platform)* Trim health-bridge comments to lean rationale

### 🐛 Bug Fixes
- *(kick)* Feed BuiltinHealth::stream() with live connection deltas
- *(youtube)* Feed BuiltinHealth::stream() with live connection deltas
- *(desktop)* Render inactive sidebar icons and label builtin section
- *(desktop)* Match sidebar icons to the design for six nav items
- *(desktop)* Always show stream health and balance the home layout
- *(ui)* Give chat rows a stable id so hover survives re-render
- *(ui)* Stop chat timestamps overlapping and brighten the founder badge
- *(ui)* Match chat composer sizing and add a dynamic placeholder
- *(ui)* Thin the panel resize divider
- *(ui)* Make the chat composer a single borderless input block
- *(ui)* Keep the chat input pinned and scroll the message list
- *(chat)* Top-anchor the message list so short chats fill from the top
- *(ui)* Make the composer platform divider visible

### 🚀 Features
- *(desktop)* Enumerate soundboard devices and tts engines live
- *(desktop)* Stream script log output to the editor console
- *(desktop)* Show live viewers and uptime in the chat header
- *(desktop)* Add a commands count to the home at-a-glance
- *(ui)* Default to Tokyo Night theme
- *(storage)* Persist chat history to a dedicated table
- *(runtime)* Drain live chat into the history store
- *(ui)* Render rich chat rows and restore history on boot
- *(ui)* Make chat history limits configurable in settings
- *(storage)* Add chat sequence id and moderation update methods
- *(runtime)* Persist chat moderation via a moderation envelope
- *(twitch)* Attach moderation envelope to delete/ban/clear events
- *(ui)* Strike through moderated messages and update live feed
- *(ui)* Redesign chat rows to single-line with right-aligned badges
- *(ui)* Add chat-log export and refine the chat chrome
- *(runtime)* Track chat viewers into the viewer store
- *(ui)* Restore the viewers drawer on the chat screen
- *(ui)* Dispatch viewer actions from the chat drawer
- *(ui)* Annotate chat event rows with the triggered action
- *(ui)* Make the viewers drawer and sidebar resizable
- *(chat)* Show viewer last-seen from their latest message
- *(chat)* Render Twitch cheers as a distinct bits event
- *(runtime)* Emit command.matched when a chat command fires
- *(chat)* Render command messages as command rows
- *(chat)* Carry Twitch reply threading via an envelope
- *(chat)* Render reply messages with a quoted parent
- *(chat)* Isolate uptime rerender and add a user moderation menu
- *(chat)* Reply to a message from the user context menu

### 🚜 Refactor
- *(desktop)* Move oauth connect flow to its own module
- *(desktop)* Collapse home jump-card args into a spec struct

### 🛠️ Build
- *(deps)* Migrate gpui to git dependency at tag v1.11.3

### 🧪 Testing
- *(platform)* Cover kick and youtube health-delta bridges
- *(chat-history)* Cover history repo, persistence loop, and converter
- *(chat)* Cover chat moderation storage, envelope, stream and feed
- *(chat)* Cover viewers-drawer data logic and viewer tracker
- *(chat)* Cover cheer body, command marking, and reply parsing

## [0.3.0-beta.3] - 2026-07-17
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump open from 5.3.6 to 5.4.0 (#41)
- *(deps)* Bump ulid from 1.2.1 to 2.0.1 (#42)
- Release

### 🎨 Styling
- Replace em and en dashes with plain hyphens
- Replace prose em-dash escapes with hyphens

### 🐛 Bug Fixes
- *(ids)* Adapt ULID generation to the ulid 2.0 API
- *(desktop)* Run dashboard stats query on the tokio runtime
- *(deps)* Drive zbus on async-io so gpui portal usage has a reactor
- *(desktop)* Hold a tokio enter guard so pool teardown has a reactor
- *(storage)* Resurrect an archived global on set

### 📚 Documentation
- *(workspace)* Drop non-invariant doc comments
- *(readme)* Correct the fourth cloud TTS engine to Amazon Polly
- *(release)* Release v0.3.0-beta.3

### 🚀 Features
- *(storage)* Gate open on a typed schema-version mismatch
- *(desktop)* Boot the real runtime behind a two-phase boot
- *(desktop)* Feed observability topics from the live event bus
- *(desktop)* Bring up the platform integrations at boot
- *(desktop)* Wire live connection state into the integration UI
- *(desktop)* Feed the Home viewers-now cell from the live aggregate
- *(desktop)* Pull Home at-a-glance stats from the dashboard aggregate
- *(desktop)* Stream live health deltas into the integration detail
- *(desktop)* Drive integration lifecycle controls through the runtime
- *(desktop)* Dispatch integration quick actions through the action engine
- *(desktop)* Pick OBS scene and source targets for quick actions
- *(server)* Expose an in-process ServerSnapshot getter on ServerHandle
- *(desktop)* Drive the server console from a live snapshot poll
- *(desktop)* Bring up the speak pipeline at boot
- *(desktop)* Drive the TTS dashboard from the live speak queue
- *(desktop)* Drive queue console controls through the scheduler
- *(storage)* Soft-delete globals via an archive marker
- *(desktop)* Drive the globals screen from live storage CRUD
- *(storage)* Soft-delete actions and trigger instances via archive markers
- *(desktop)* Drive the actions roster from live storage CRUD
- *(desktop)* Rewrite the action editor onto live sub-action steps
- *(desktop)* Show linked trigger instances in the action editor
- *(desktop)* Fire a synthetic test event from the action editor
- *(desktop)* Drive the triggers roster from live storage CRUD
- *(desktop)* Show trigger detail with a live config editor
- *(desktop)* Create trigger instances from a kind picker
- *(desktop)* Link and unlink triggers from the action editor
- *(desktop)* Drive the queues roster from storage with live register
- *(desktop)* Drive voice aliases from storage with live hot-reload
- *(desktop)* Drive tts filters from storage with live pipeline swap
- *(desktop)* Register audio sub-action runners in the runtime
- *(desktop)* Persist tts trigger settings with live hot-swap
- *(desktop)* Store cloud tts credentials with live engine register
- *(desktop)* Drive the tts engines roster from the live registry
- *(desktop)* Write rolling file logs via a tracing subscriber
- *(components)* Add a fluent-backed localization runtime
- *(desktop)* Resolve and install the startup locale at boot
- *(desktop)* Localize the navigation sidebar labels
- *(desktop)* Localize the home, platforms, and stream apps screens
- *(desktop)* Localize the live chat and event feed screens
- *(desktop)* Localize the globals screen
- *(desktop)* Localize the queues screen
- *(desktop)* Localize the actions list screen
- *(desktop)* Localize the action editor screen
- *(desktop)* Localize the branch flow-control editor
- *(desktop)* Localize the triggers list and detail screens
- *(desktop)* Localize the trigger create screen
- *(desktop)* Localize the TTS dashboard screen
- *(desktop)* Localize the TTS engines screens
- *(desktop)* Localize the TTS triggers and voice aliases screens
- *(desktop)* Localize the TTS filters screen
- *(desktop)* Localize the soundboard and server console screens
- *(desktop)* Localize the script editor screen
- *(desktop)* Localize the integration detail renderer
- *(desktop)* Localize integration preview seed labels
- *(desktop)* Localize the boot and data-failure screens
- *(desktop)* Localize the settings frame and panes
- *(desktop)* Localize the trigger platform filter chips
- *(desktop)* Port the settings language section
- *(desktop)* Port the settings audio section
- *(desktop)* Port the settings notifications, queues, and storage panes
- *(desktop)* Port the settings scripting section
- *(desktop)* Port the settings websocket server section
- *(desktop)* Port the settings shortcuts section
- *(desktop)* Wire the global-hotkey subsystem into boot
- *(desktop)* Port the settings hotkeys section
- *(desktop)* Wire file-dialog import and export handlers
- *(desktop)* Render scripting api docs from the live catalog
- *(desktop)* Connect youtube and kick via loopback oauth
- *(desktop)* Connect twitch via device-code authorization
- *(desktop)* Play and persist soundboard clips via runtime
- *(desktop)* Run and persist scripts via the runtime

### 🚜 Refactor
- *(components)* Promote field_label to the kit
- *(components)* Let field_label take dynamic label text
- *(desktop)* Hold runtime handles in the settings view

### 🛠️ Build
- *(deps)* Bump open to 5.4
- *(deps)* Bump version to nearest compatible
- *(desktop)* Make gpui the shipped forge binary, remove iced
- *(deps)* Bump versions
- *(audit)* Ignore unreachable quick-xml advisories in the gpui stack
- *(linux)* Add libxkbcommon-x11 build and runtime deps for gpui
- Install gpui system libraries in the shared rust setup
- *(deps)* Bump ulid to 3.0.0

### 🧪 Testing
- *(storage)* Cover soft-delete archive/restore for globals/actions/triggers
- *(i18n)* Guard ftl key parity and locale formatters

## [0.3.0-beta.2] - 2026-07-13
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(desktop)* Route midi/hotkey/discord modules to integration detail

### 📚 Documentation
- *(release)* Release v0.3.0-beta.2

### 🚀 Features
- *(desktop)* Boot the gpui app shell with fonts, theme global, and screen router
- *(desktop)* Add the sidebar nav, footer, and title bar chrome
- *(desktop)* Add the live chat screen with a seeded message feed
- *(desktop)* Add the home dashboard screen
- *(desktop)* Add the event feed screen
- *(desktop)* Add the globals variables screen
- *(components)* Add full-width button builder
- *(desktop)* Add the settings screen
- *(desktop)* Add the platforms overview screen
- *(desktop)* Add the generic integration detail screen
- *(desktop)* Add the stream apps overview screen
- *(components)* Add explicit width override to modal
- *(desktop)* Add the soundboard screen
- *(desktop)* Add the queues screen
- *(desktop)* Add the tts screen frame and dashboard
- *(desktop)* Add the tts engines section
- *(desktop)* Add the tts voice aliases section
- *(desktop)* Add the tts filters section
- *(desktop)* Add the tts triggers section
- *(desktop)* Add the tts cloud engines section
- *(desktop)* Add the websocket server console screen
- *(desktop)* Add the actions screen list pane
- *(desktop)* Add the actions editor step list
- *(desktop)* Add the actions trigger links and picker
- *(desktop)* Add the actions branch drill-in editing
- *(desktop)* Add the triggers registry list and filters
- *(desktop)* Add the triggers registry config side-sheet
- *(desktop)* Add the triggers registry create picker
- *(desktop)* Add the script editor screen
- *(desktop)* Add the script editor api reference and run modal
- *(components)* Add a secure masked mode to text input
- *(desktop)* Add the global toast host
- *(components)* Add a sparkline chart and wire throughput cards

### 🚜 Refactor
- *(desktop)* Unify add-step and add-trigger into a grid picker
- *(components)* Promote the add picker to a shared grid component
- *(desktop)* Split the actions screen into submodules

## [0.3.0-beta.1] - 2026-07-12
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump crossbeam-channel from 0.5.15 to 0.5.16 (#40)
- *(deps)* Bump tokio-tungstenite, zbus, rubato, and regex
- Release

### 🐛 Bug Fixes
- *(widgets)* Drop private intra-doc link on RowCard::selected
- *(audio)* Adapt the resampler call to the rubato 4.0 process signature

### 📚 Documentation
- *(release)* Release v0.3.0-beta.1

### 🚀 Features
- *(ui)* Scaffold the gpui forge-components and forge-desktop crates
- *(desktop)* Bridge runtime timer ticks into the gpui uptime view
- *(ui)* Port the design tokens into the gpui component kit
- *(ui)* Add the status-dot indicator to the component kit
- *(ui)* Add the tabler icon system to the component kit
- *(ui)* Add the badge family to the component kit
- *(ui)* Add the chip family to the component kit
- *(ui)* Add the button family to the component kit
- *(ui)* Add the card family to the component kit
- *(ui)* Add the interactive row-card to the component kit
- *(ui)* Add the breadcrumb to the component kit
- *(ui)* Add the app footer to the component kit
- *(ui)* Add the side-sheet panel to the component kit
- *(ui)* Add the overlay primitive to the component kit
- *(ui)* Add the base modal card to the component kit
- *(ui)* Add the confirm modal to the component kit
- *(ui)* Add the toggle switch to the component kit
- *(ui)* Add the slider to the component kit
- *(ui)* Add the single-line text input to the component kit
- *(ui)* Add the search input to the component kit
- *(ui)* Add the multi-line text area to the component kit
- *(ui)* Add the type-to-confirm modal to the component kit
- *(ui)* Add the picker modal to the component kit
- *(ui)* Add the data table to the component kit
- *(ui)* Add the overflow menu to the component kit
- *(ui)* Add the chat row to the component kit
- *(ui)* Add the chat input bar to the component kit
- *(ui)* Add the tooltip to the component kit

### 🧪 Testing
- *(components)* Cover icon from_name aliases and asset loading
- *(components)* Cover StatusVariant colors hue mapping and tint
- *(components)* Pin chip active-state fill and ink resolution
- *(components)* Cover button variant color resolution
- *(components)* Cover row-card color resolution across states
- *(components)* Cover version-stage first-dash split
- *(components)* Pin ease_out_cubic curve shape and monotonicity
- *(components)* Pin confirm tone accent to severity palette field
- *(components)* Pin toggle_colors state-to-field mapping and accent gating
- *(components)* Pin slider fraction/value_at clamping and inverse
- *(components)* Cover grapheme-aware text-input editing
- *(components)* Cover multi-line text-area editing
- *(components)* Pin type-to-confirm match, border and bullet mapping
- *(components)* Pin picker item_matches filter semantics
- *(components)* Pin column_flex fixed-vs-flex flexbox mapping
- *(components)* Pin menu actionable_count filtering
- *(components)* Map chat-row badge/platform hues and triggered fan-in
- *(components)* Cover input-bar target-selection bitset logic

## [0.2.0-beta.3] - 2026-07-11
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(triggers)* Store created instance config as sparse override diff
- *(home)* Thin row dividers and hairline integration cell gaps
- *(home)* Space glance header and divide every glance row
- *(home)* Render glance row separators at one physical pixel
- *(chat)* Trim breadcrumb to Chat and relabel events filter
- *(chat)* Add users icon to viewers drawer header and drop its close X
- *(chat)* Size the search field to chip height so the bar stops jumping
- *(chat)* Make viewers drawer header a compact inline title with count
- *(ui)* Paint side sheets with the crust surface to match panels
- *(chat)* Use the elevated surface for filter bar and composer field
- *(chat)* Match viewers drawer surfaces to design elevation
- *(chat)* Unify feed and drawer badges as solid role fills
- *(chat)* Scale drawer meta text down to the design font sizes
- *(ui)* Tighten Xs and Sm spacing to the design scale
- *(ui)* Size search input text to the design scale
- *(chat)* Render the viewers drawer as a side panel not an overlay
- *(chat)* Match drawer badge shape, section divider, and button hover
- *(chat)* Show all platform chips and the audience breadcrumb
- *(chat)* Match composer placeholder and platform pill to the design
- *(actions)* Inline section counts, add trigger hint, scale meta text
- *(actions)* Reconcile badge, cards, chips, search, and hovers to design
- *(actions)* Bucket actions by their custom group name
- *(actions)* Fix nav icon, chip glyphs, card hover/click, X hover, button weight

### 📚 Documentation
- *(release)* Release v0.2.0-beta.3

### 🚀 Features
- *(widgets)* Add primary-CTA empty state variant
- *(viewers)* Add live viewer-count aggregate foundation
- *(twitch)* Report live viewer count to the aggregate
- *(youtube)* Poll and report live concurrent viewers
- *(kick)* Poll and report live stream viewers
- *(home)* Show live viewers-now figure on the audience card
- *(chat)* Show live viewers and uptime in the page header
- *(chat)* Add toggleable message search in the filter bar
- *(settings)* Name the default theme Default
- *(chat)* Send to a multi-select of platform targets

### 🚜 Refactor
- *(ui)* Reskin Triggers registry to match design
- *(ui)* Reskin Action Editor rows and rename glyph
- *(ui)* Reskin Event Feed chips, toolbar, and inspector
- *(ui)* Reskin Globals with hover rows and multiline editor
- *(ui)* Reskin TTS Filters stages and Voice Aliases
- *(ui)* Reskin Script Editor toolbar, pills, and API catalog
- *(ui)* Reskin toggle accents, toast stacking, and modals
- *(ui)* Reskin Settings info rows and add server token caveat
- *(ui)* Reskin shell nav grouping, hover guard, and footer
- *(ui)* Reskin Chat badges, cheer icon, and filter bar
- *(nav)* Match sidebar structure to design

### 🧪 Testing
- *(widgets)* Cover theme-id round-trip, vip badge distinctness, scope namespace
- *(chat)* Cover display_text mention-split and extended badge set_ids
- *(chat)* Pin moderation step config-key contract and prune tautologies
- *(triggers)* Pin sparse override diff prunes revert-to-default
- *(runtime)* Pin live-viewer aggregate sum/absent/drop/empty
- *(platforms)* Cover viewer-poll live/absent/transient mapping

## [0.2.0-beta.2] - 2026-07-05
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(widgets)* Fire popover menu item click before dismissing menu
- *(ui)* Keep modal open when clicking inside its card

### 🚀 Features
- *(ui)* Add theme picker with persistence
- *(ui)* Add slider primitive and adopt it in TTS
- *(ui)* Add multiline text_area field and adopt it
- *(ui)* Add skeleton loader and fix false-empty registry flicker
- *(ui)* Add inline rename to globals and script rows
- *(ui)* Add Esc and Ctrl+Enter keyboard affordance to overlays

### 🚜 Refactor
- *(ui)* Reskin Actions detail pane to match design
- *(ui)* Reskin Actions list rows with hover overflow menu
- *(ui)* Unify toggle switches on one sized primitive
- *(obs)* Swap OBS panel toggle onto shared switch primitive
- *(ui)* Unify modal chrome with size and icon-tile slots
- *(ui)* Unify page headers on the breadcrumb primitive
- *(ui)* Consolidate status footers on one primitive
- *(widgets)* Add canonical state_icon dispatcher
- *(ui)* Fold badge builders into one and unify platform identity
- *(ui)* Consolidate filter chips on one primitive
- *(ui)* Make card a flexible builder and adopt on Home
- *(ui)* Collapse overview cards and converge metric sizing
- *(ui)* Adopt shared card across TTS surfaces
- *(ui)* Add shared row_card primitive and adopt it
- *(ui)* Reskin Home health strip and event rows
- *(ui)* Reskin Platforms and letter-tile the builtin hero

### 🧪 Testing
- *(tts)* Enable twitch emote source in stage-order fixtures

## [0.2.0-beta.1] - 2026-07-04
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(ui)* Align palette hexes with per-theme design values
- *(ui)* Tighten button padding, unify radius and label size
- *(ui)* Align input backgrounds, padding, and focus ring with design
- *(twitch)* Wire quick actions to real shoutout/ad/title runners
- *(ui)* Scroll event feed to bottom when auto-scroll is on
- *(tts)* Wire dashboard now-speaking, queue, and live volume
- *(script)* Enforce persisted op-limit and timeout at execution
- *(tts)* Hot-register cloud engines and reflect membership in badge
- *(server)* Re-apply persisted settings on server restart
- *(runtime)* Test-run fires trigger once and honors its phrase
- *(ui)* Dim disabled trigger instances in action editor
- *(runtime)* Honor configured persisted flag in globals runners
- *(app)* Revert globals persist toggle on write failure
- *(audio)* Persist output device preference and resolve it at boot
- *(hotkey)* Re-register persisted bindings at boot
- *(ui)* Remove dead trigger-row overflow glyph
- *(storage)* Update globals persisted flag without bumping writes
- *(app)* Stop inflating global writes counter on persist toggle
- *(widgets)* Delete zero-consumer client-row and overlay-file-list widgets
- *(tts)* Gate general emote strip on strip_twitch_emotes
- *(ui)* Gate trigger-instance delete behind confirm modal
- *(ui)* Reachable globals delete with confirm and undo
- *(ui)* Gate Actions delete/unlink behind shared confirm modal
- *(ui)* Make script delete reachable and confirm-gated, add rename
- *(ui)* Gate last 4 destructive actions behind confirm modal
- *(storage)* Make global rename atomic and collision-checked
- *(ui)* Validate global names and lock kind on edit
- *(ui)* Validate script body and guard dirty-state on navigation
- *(storage)* Copy trigger-instance links on action duplicate
- *(ui)* Surface export/replay/script/trigger-delete failures as toasts
- *(widgets)* Add toast drop-shadow and undo accent color
- *(twitch)* Feed live health deltas instead of dropping the sender
- *(youtube)* Wire connected-state builtin bundle end to end
- *(ui)* Feed connectivity resolver to sidebar dots and platform cards
- *(ui)* Feed home throughput sparkline real ev/s samples
- *(ui)* Read real emitter fields in event summaries
- *(ui)* Walk caused_by edge for event feed CAUSED section
- *(runtime)* Preserve nested branch/loop/switch step telemetry
- *(ui)* Toast chat/TTS async outcomes and guard Replay double-fire
- *(ui)* Resolve triggered-action chat badge via caused_by correlation
- *(ui)* Carry youtube member level and milestone into chat row
- *(tts)* Source engine and voice pickers from the live registry
- *(chat)* Render mentions inline and cover the full badge roster

### 🚀 Features
- *(ui)* Add Inter Medium/SemiBold font weight axis
- *(tts)* Wire voice-alias resolver to strategy and CRUD
- *(tts)* Persist TTS trigger source toggles
- *(tts)* Gate speak sub-action by message source toggles
- *(tts)* Strip emote tokens from reward-sourced speech
- *(widgets)* Add shared destructive-confirm modal primitive
- *(ui)* Thread undo action through ToastMsg::Fired contract
- *(obs)* Live metrics, cold-connect status, and catalog fixes
- *(widgets)* Add status dot to sidebar FlatLink
- *(twitch)* Poll live viewer count into health metrics
- *(script)* Emit script.log bus events and count forge::error calls
- *(ui)* Stream script console output from the event bus
- *(ui)* Load Home stats on cold boot, surface failures, wire Import
- *(triggers)* Add row overflow menu with rename and use-as-template
- *(triggers)* Make instance config editable with per-field revert
- *(tts)* Add block-from-TTS toggle to voice alias form
- *(chat)* Dispatch timeout, ban, and TTS-block from viewer drawer
- *(ui)* Add Discord, MIDI, Hotkey sidebar entries
- *(settings)* Render version card and recent releases

### 🚜 Refactor
- *(ui)* Unify bespoke dividers onto shared primitive
- *(server)* Remove dead server-control update-arms
- *(settings)* Remove vacuum button absent from design
- *(ui)* Derive connectivity from one shared 5-roster resolver
- *(ui)* Remove dead Logs screen and sidebar-group scaffolding
- *(widgets)* Extract shared platform identity letter-tile

### 🛠️ Build
- *(deps)* Bump notify-rust to 4.18

### 🧪 Testing
- *(ui)* Pin palette structural tokens to per-theme design values
- *(script)* Guard persisted op-limit load and enforcement
- *(tts)* Guard speak volume clamp, payload, and resolver commands
- *(tts)* Guard cloud engine badge registry-membership derivation
- *(runtime)* Guard record store-only with single replay delivery
- *(app)* Cover configured phrase in synthesized test event
- *(storage)* Cover globals persisted query and flag round-trip
- *(runtime)* Preserve persisted flag across globals runners
- *(app)* Revert globals persist toggle on write failure
- *(runtime)* Cover speak source-gating classify and toggle seam
- *(app)* Prune tautological trigger-synthesis tests
- *(hotkeys)* Cover stale hotkey-combo orphan cleanup
- *(ui)* Pin cheer detail renders as inline cheer body
- *(runtime)* Cover nested branch/loop/switch step telemetry tagging
- *(script)* Cover script.log emission and forge::error counting

## [0.1.0] - 2026-07-03
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(app)* Show honest placeholders for unknown viewer stats
- *(platform)* Serialize PlatformId::YouTube as "youtube"
- *(app)* Emit canonical chat.send success kind for kick/youtube
- *(chat)* Disable unimplemented viewer-menu actions
- *(server)* Release inner mutex before drain in ServerHandle::stop
- *(server)* Snapshot client rows before async subscription lookups
- *(server)* Bind and shut down server outside the handle write lock
- *(twitch)* Release OAuth flow lock during authorization wait
- *(server)* Snapshot clients before async subscription lookups on metrics tick
- *(server)* Clone ServerHandle out of the lock before accessor awaits
- *(storage)* Prune action history and executions in retention task
- *(ui)* Align radius and font scales with design tokens

### 🚀 Features
- *(platform)* Add connection-state event and shared-self chat trait
- *(twitch)* Realize ChatPlatform with owned event stream
- *(kick)* Realize ChatPlatform with owned event stream
- *(youtube)* Realize ChatPlatform with poller-backed lifecycle
- *(storage)* Add ActionRepo::record_execution production write path
- *(runtime)* Record action executions to telemetry table

### 🚜 Refactor
- *(storage)* Remove orphaned test-only execution insert helper

### 🛠️ Build
- *(deps)* Bump aes-gcm to 0.11 and refresh lockfile

### 🧪 Testing
- *(kick)* Cover ChatPlatform lifecycle, send-reauth and control delegation
- *(youtube)* Cover ChatPlatform connect/disconnect transitions and dedup
- *(types)* Assert exact wire strings for all PlatformId variants
- *(app)* Add chat.send mapping regression tests in event_feed
- *(storage)* Cover record_execution telemetry round-trip
- *(runtime)* Cover ActionEngine telemetry write path
- *(twitch)* Cover one-shot OAuth flow consumption
- *(server)* Cover stop idempotency and client-snapshot introspection
- *(storage)* Cover retention pruning of action_history and executions
- *(storage)* Cover decrypt rejecting wrong-length nonce

## [0.1.0-beta.16] - 2026-06-30
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump actions/cache from 5 to 6 (#36)
- *(deps)* Bump arc-swap from 1.9.1 to 1.9.2 (#38)
- *(deps)* Bump mockall from 0.14.0 to 0.15.0 (#39)
- *(deps)* Bump open from 5.3.5 to 5.3.6 (#37)
- Release

### 🐛 Bug Fixes
- *(runtime)* Cancel in-flight action on queue clear without keep-current
- *(runtime)* Execute flow-control inline sub-chains
- *(app)* Show add-sub-action modal on action editor screen
- *(app)* Populate action/queue/trigger/script select options
- *(app)* Make the action editor reachable from actions
- *(app)* Anchor action context menu to its trigger button
- *(app)* Add script editor to the sidebar navigation
- *(app)* Complete sidebar nav and drop dead builtin screen

### 📚 Documentation
- *(readme)* Document expanded sub-actions and flow control
- *(release)* Release v0.1.0-beta.16

### 🚀 Features
- *(runtime)* Add re-entrant chain executor with control-flow signals
- *(runtime)* Add bounded condition evaluator with literal fast-path
- *(audio)* Add stoppable playback handle for in-flight clips
- *(tts)* Add speak queue control and query dispatch surface
- *(runtime)* Add string manipulation sub-action runners
- *(runtime)* Add math and random sub-action runners
- *(runtime)* Add datetime sub-action runners
- *(runtime)* Add sandboxed file write, delete and list sub-actions
- *(runtime)* Add globals decrement, toggle and array sub-actions
- *(runtime)* Add local arg set sub-action runner
- *(runtime)* Add per-user variable sub-action runners
- *(runtime)* Add queue pause, resume and clear sub-actions
- *(runtime)* Add custom event emit sub-action runner
- *(runtime)* Add server broadcast sub-action runner
- *(runtime)* Add notify, clipboard and url-open sub-actions
- *(runtime)* Add trigger test-fire sub-action runner
- *(runtime)* Add HTTP egress sub-actions with SSRF guard
- *(tts)* Add TTS queue control and alias sub-actions
- *(soundboard)* Add stop, stop-all and master-volume sub-actions
- *(runtime)* Add action and trigger enable/disable sub-actions
- *(runtime)* Add re-entrant action.run with RunContext executor
- *(runtime)* Add flow-control composite and signal sub-actions
- *(runtime)* Add action.cancel to stop in-flight action runs
- *(actions)* Author flow-control nested sub-chains in editor

### 🚜 Refactor
- *(net)* Relocate SSRF address classifier to shared crate
- *(actions)* Drop superseded sub-action service methods
- *(app)* Consolidate action list and editor into one screen

### 🧪 Testing
- *(runtime)* Cover chain executor depth, cancel and signal mapping
- *(runtime)* Cover bounded condition evaluator and fast-path parity
- *(tts)* Cover speak queue control and query dispatch surface
- *(runtime)* Cover string sub-action runner edge cases
- *(runtime)* Cover math/random sub-action runners and MathEvaluator
- *(runtime)* Cover datetime sub-action runners
- *(runtime)* Cover sandboxed file sub-actions, prune moved tests
- *(runtime)* Cover globals decrement, toggle and array sub-actions
- *(runtime)* Cover per-user variable and local-arg set runners
- *(runtime)* Cover queue control runners via scheduler cell
- *(runtime)* Cover script.emit_event custom-event round-trip
- *(runtime)* Cover server.broadcast overlay round-trip contract
- *(runtime)* Cover notify/clipboard/url-open runners + url scheme gate
- *(runtime)* Cover trigger test-fire runner dispatch and outputs
- *(net)* Cover SSRF address classifier at shared crate home
- *(runtime)* Cover HTTP egress client and core.http runners
- *(tts)* Cover TTS control runner dispatch and arg marshaling
- *(soundboard)* Cover stop, stop-all and master-volume sub-actions
- *(runtime)* Cover action/trigger enable/disable/toggle state runners
- *(runtime)* Cover action.run composite and RunContext leaf
- *(runtime)* Cover flow-control sub-action runners
- *(runtime)* Cover action.cancel runner and cancel registry
- *(actions)* Cover nested sub-chain nav and switch cases
- *(app)* Cover action-screen consolidation routing
- *(app)* Guard Instant subtraction against Windows underflow

## [0.1.0-beta.15] - 2026-06-26
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump actions/checkout from 6 to 7 (#33)
- *(deps)* Update to latest compatible and fix time deprecation
- Release

### 🐛 Bug Fixes
- *(ui)* Resolve trigger labels and picker groups from registry
- *(ui)* Localize picker sub-groups and complete platform filter chips
- *(storage)* Rename kind-ids in place so FK-linked defaults survive
- *(ui)* Surface database-open failure on an error screen
- *(tts)* Stop-all clears the speak queue, not just the UI list
- *(scheduler)* Preserve pause across a blocking-flip reconfigure
- *(deps)* Bump quinn-proto and memmap2 to clear security advisories

### 🚀 Features
- *(obs)* Add scene-family triggers and explicit event subscription
- *(obs)* Add streaming lifecycle triggers
- *(obs)* Add recording lifecycle triggers
- *(obs)* Add studio-mode and transition triggers
- *(obs)* Add audio source triggers
- *(obs)* Add input lifecycle and scene-item visibility triggers
- *(obs)* Add scene-item lock-state trigger
- *(obs)* Add source-filter triggers
- *(obs)* Add scene-lifecycle and profile/collection-list triggers
- *(obs)* Add virtual-camera triggers
- *(obs)* Add connection lifecycle triggers
- *(obs)* Add preview-scene, transition, volume and input-settings sub-actions
- *(obs)* Add recording-pause, resume, toggle and stream-caption sub-actions
- *(obs)* Add replay-buffer and studio-mode sub-actions
- *(obs)* Add scene/source/status/settings lookup sub-actions
- *(vtube)* Add model, hotkey, and expression triggers
- *(vtube)* Add face-tracking and item triggers
- *(vtube)* Add move-item sub-action runner
- *(vtube)* Add state-lookup sub-action runners
- *(midi)* Add pitch-bend and program-change triggers
- *(midi)* Add device-connected and device-disconnected triggers
- *(discord)* Add send-file and delete-message webhook runners
- *(storage)* Migrate persisted midi/hotkey/discord kind-ids
- *(ui)* Add New Queue creation modal
- *(ui)* Wire voice-alias assign, edit, delete, and play preview
- *(ui)* Wire TTS dashboard transport and filter speak preview
- *(ui)* Wire overlay-folder Browse to a native picker
- *(ui)* Wire live-chat Shoutout and Whisper to Twitch sub-actions
- *(ui)* Reveal the overlay folder from the server screen
- *(ui)* Add a Configure modal to rename and re-flag a queue
- *(integrations)* Add BuiltinControl lifecycle trait
- *(obs)* Implement BuiltinControl reconnect and disconnect
- *(vtube)* Implement BuiltinControl reconnect and disconnect
- *(twitch)* Implement BuiltinControl for the Twitch integration
- *(kick)* Implement BuiltinControl with genuine token refresh
- *(ui)* Wire integration header actions to BuiltinControl
- *(storage)* Persist TTS filter rules and pipeline settings
- *(tts)* Add hot-reloadable pipeline config and filter mapping
- *(tts)* Wire filters screen to storage and live pipeline
- *(runtime)* Add live queue register, deregister, reconfigure
- *(queues)* Register and reconfigure queues with the live scheduler
- *(kick)* Broadcast chat connection state via watch channel
- *(kick)* Expose lifecycle controls in the integration detail
- *(kick)* Expose the working token refresh as a header action
- *(twitch)* Refresh OAuth tokens proactively and on 401
- *(twitch)* Wire the token refresher into the Helix transport
- *(twitch)* Make the Helix token source proactively refresh

### 🚜 Refactor
- *(midi)* Rename input trigger kind-ids to catalog scheme
- *(discord)* Rename webhook sub-action kind-ids to catalog scheme
- *(hotkey)* Rename pressed-trigger kind-id to catalog scheme

### 🧪 Testing
- *(obs)* Cover scene-family trigger matching and arg stacks
- *(obs)* Cover streaming lifecycle trigger discrimination
- *(obs)* Cover recording lifecycle trigger discrimination
- *(obs)* Cover studio-mode and transition trigger discrimination
- *(obs)* Cover audio source trigger family
- *(obs)* Cover source input lifecycle and scene-item visibility triggers
- *(obs)* Cover scene-item lock-state trigger
- *(obs)* Cover source-filter trigger family
- *(obs)* Cover scene-lifecycle and profile/collection-list trigger discrimination
- *(obs)* Cover virtual-cam and connection-lifecycle triggers
- *(obs)* Consolidate runner mock and cover new sub-actions
- *(obs)* Cover record pause/resume/toggle and stream caption runners
- *(obs)* Cover no-config replay and studio sub-action runners
- *(obs)* Cover lookup sub-action arg-stack extraction
- *(vtube)* Cover model/hotkey/expression trigger match and args
- *(vtube)* Cover tracking and item triggers + dispatch
- *(vtube)* Consolidate runner mock sinks into shared test double
- *(vtube)* Cover item-move noop, dispatch, fade, interpolation
- *(vtube)* Cover lookup sub-action arg-stack extraction
- *(app)* Cover all vtube sub-action runners in boot wiring
- *(midi)* Cover pitch-bend and program-change decode + triggers
- *(midi)* Cover device hotplug trigger matching and arg stack
- *(discord)* Cover send-file and delete-message webhook runners
- *(storage)* Cover discord blob remap and midi/hotkey id migration
- *(ui)* Cover trigger-picker grouping and sub-group labels
- *(storage)* Cover FK-linked default survives kind-id rename
- *(ui)* Cover New Queue modal state transitions and empty-name guard
- *(tts)* Cover voice-alias form, delete-gate, and preview state machine
- *(integrations)* Cover BuiltinControl token-safety and missing-creds guards
- *(storage)* Cover TTS filter repo round-trips and migration
- *(tts)* Cover filter mapping and hot-reload handle
- *(tts)* Cover filters screen update logic
- *(scheduler)* Cover live-membership register, deregister, reconfigure
- *(scheduler)* Regress blocking-flip preserves pause
- *(queues)* Cover live-membership badge transitions and pruning
- *(twitch)* Cover token refresh round-trip, back-compat, rotation

## [0.1.0-beta.14] - 2026-06-18
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(ui)* Group kick triggers under a kick picker section
- *(ui)* Color youtube and kick sub-action category pills
- *(kick)* Keep chat receive loop alive after connect
- *(kick)* Expose message_id and repair delete-message default
- *(kick)* Point reward-id hints at the resolvable arg
- *(kick)* Dedupe redemptions against the live pending set

### 📚 Documentation
- *(readme)* Describe kick official write tier and triggers
- *(release)* Release v0.1.0-beta.14

### 🚀 Features
- *(storage)* Migrate kick trigger kind_ids to canonical scheme
- *(kick)* Open write tier — full oauth scopes + moderation/rewards caps
- *(kick)* Add chat send + delete sub-action runners
- *(kick)* Add ban/timeout/unban moderation sub-action runners
- *(kick)* Add channel update-info sub-action runner
- *(kick)* Add channel-reward create/update/delete sub-action runners
- *(kick)* Add redemption accept/reject sub-action runners
- *(kick)* Add chat.command trigger descriptor
- *(kick)* Add livestream status, metadata, redemption triggers
- *(kick)* Add channel and redemption GET poll read methods
- *(kick)* Add livestream and redemption poll loop task
- *(app)* Wire kick sub-actions and poll loop into runtime

### 🚜 Refactor
- *(kick)* Rename trigger ids to catalog-canonical scheme
- *(kick)* Run poll loop detached without shutdown handle

### 🧪 Testing
- *(storage)* Cover kick kind_id rename data migration
- *(kick)* Cover chat send/delete runners + write scopes
- *(kick)* Cover ban/timeout/unban moderation runners and client
- *(kick)* Cover channel update-info runner and transport
- *(kick)* Cover reward CRUD client and runner guards
- *(kick)* Cover redemption accept/reject batching and guards
- *(kick)* Cover chat-command trigger prefix match and arg split
- *(kick)* Cover poll-trigger arg-stack nested field extraction
- *(kick)* Cover channel and redemption GET poll read methods
- *(kick)* Cover poller dedupe, channel diff, and emit gating

## [0.1.0-beta.13] - 2026-06-16
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump tower-http from 0.6.11 to 0.7.0 (#28)
- *(deps)* Bump fluent from 0.16.1 to 0.17.0 (#29)
- *(deps)* Bump regex from 1.12.3 to 1.12.4 (#30)
- *(deps)* Bump cpal from 0.18.0 to 0.18.1 (#32)
- *(deps)* Bump fluent-langneg from 0.13.1 to 0.14.2 (#31)
- Release

### 🐛 Bug Fixes
- *(deps)* Pin fluent-langneg to 0.13 to match fluent stack
- *(ui)* Show youtube triggers in their own picker group

### 📚 Documentation
- *(readme)* Expand youtube trigger and sub-action coverage
- *(release)* Release v0.1.0-beta.13

### 🚀 Features
- *(storage)* Migrate youtube trigger kind_ids to canonical scheme
- *(youtube)* Add chat send-message sub-action runner
- *(youtube)* Add ban, timeout, and unban sub-action runners
- *(youtube)* Add moderator add and remove sub-action runners
- *(youtube)* Add chat delete-message sub-action runner
- *(youtube)* Add stream metadata update sub-action runners
- *(youtube)* Add message-deleted and membership-gift triggers
- *(youtube)* Add stream title-changed trigger

### 🚜 Refactor
- *(youtube)* Rename trigger ids to catalog-canonical scheme
- *(youtube)* Consolidate ban and timeout into user_banned trigger

### 🧪 Testing
- *(storage)* Cover youtube kind_id rename migration
- *(youtube)* Cover chat send-message sub-action runner
- *(youtube)* Cover ban, timeout, and unban moderation runners
- *(youtube)* Cover moderator add and remove runners
- *(youtube)* Cover chat delete-message sub-action runner
- *(youtube)* Cover stream metadata update runners and merge contract
- *(youtube)* Cover message-deleted and membership-gift triggers
- *(youtube)* Cover stream title-changed trigger
- *(ui)* Cover youtube trigger group in picker dispatch

## [0.1.0-beta.12] - 2026-06-15
### ⚠️ BREAKING CHANGES
- **twitch**: stored Twitch tokens lack new scopes; reauth at next connect
- **twitch**: create-marker needs new user:manage:broadcast OAuth scope

### ⚙️ Miscellaneous Tasks
- *(deps)* Bump actions/cache from 4 to 5 (#27)
- Release

### 🎨 Styling
- *(twitch)* Apply rustfmt to reward toggle runners

### 🐛 Bug Fixes
- *(twitch)* Count chat-message length in chars not bytes
- *(i18n)* Render feed-time pattern as literal text not Fluent refs
- *(twitch)* Correct cheer and shared-chat fields in chat events
- *(twitch)* Categorize sub-actions by domain not platform
- *(twitch)* Categorize follow, stream, points, hype, charity triggers
- *(app)* Resolve queue descriptions on the render thread
- *(ci)* Build release notes from prev-tag range not --latest
- *(storage)* Persist credential key in file not OS keyring
- *(ui)* Align credential notice with local encryption
- *(actions)* Drive sub-action picker from the runner registry

### 📚 Documentation
- *(readme)* Expand twitch trigger and sub-action coverage
- *(release)* Release v0.1.0-beta.12

### 🚀 Features
- *(twitch)* Route EventSub notifications by subscription type
- *(registry)* Add moderation and channel-points trigger categories
- *(twitch)* Route Helix calls through shared mockable transport
- *(twitch)* Add Helix sub-action path with chat announcement runner
- *(twitch)* Add cheer-message and shared-chat-message triggers
- *(twitch)* [**breaking**] Request full-tier OAuth scopes for runners and triggers
- *(twitch)* Add delete-message, clear, and set-mode sub-actions
- *(twitch)* Add ban, timeout, unban, and warn sub-actions
- *(twitch)* Add moderator, VIP, and shield-mode sub-actions
- *(twitch)* Add chat reply and whisper sub-actions
- *(twitch)* [**breaking**] Add channel-info and stream-marker sub-actions
- *(twitch)* Add shoutout and raid sub-actions
- *(twitch)* Add run-ad and snooze-ad sub-actions
- *(twitch)* Add create-reward channel-points sub-action
- *(twitch)* Add update-reward channel-points sub-action
- *(twitch)* Add reward enable, disable, pause, resume sub-actions
- *(twitch)* Add delete-reward and redemption-status sub-actions
- *(twitch)* Enforce Helix rate-limit budget with shared token bucket
- *(twitch)* Add poll start and end sub-actions
- *(twitch)* Add prediction start sub-action
- *(twitch)* Add prediction lock, cancel, and resolve sub-actions
- *(twitch)* Add get-current-goal sub-action
- *(twitch)* Add automod approve and deny message sub-actions
- *(twitch)* Add automod update-settings sub-action
- *(twitch)* Add automod blocked-term add and remove sub-actions
- *(twitch)* Add guest-star invite, assign, and remove sub-actions
- *(twitch)* Add guest-star update-slot and end-session sub-actions
- *(twitch)* Wire follow and stream online/offline triggers
- *(twitch)* Wire channel-points redemption trigger
- *(twitch)* Wire chat message-deleted and cleared triggers
- *(twitch)* Wire hype train begin/progress/end triggers
- *(twitch)* Wire charity donation and campaign lifecycle triggers
- *(twitch)* Wire ban, timeout, and unban triggers
- *(twitch)* Wire moderator add/remove and shield mode triggers
- *(twitch)* Wire shoutout, suspicious-user, and warning triggers
- *(twitch)* Wire poll begin/progress/end triggers
- *(twitch)* Wire prediction begin/progress/lock/end triggers
- *(twitch)* Wire goal begin/progress/end triggers
- *(twitch)* Wire reward CRUD and redemption-updated triggers
- *(twitch)* Wire automod message-held and chat-settings triggers
- *(twitch)* Wire guest-star session and settings triggers
- *(twitch)* Wire automod settings/terms/message-updated triggers
- *(twitch)* Wire shared-chat session begin/end/update triggers
- *(twitch)* Wire channel-update, ad-break, auto-reward triggers
- *(twitch)* Wire guest-star guest-updated trigger
- *(twitch)* Wire raid-sent trigger with direction discriminator
- *(twitch)* Wire vip add and remove triggers
- *(twitch)* Wire unban-request create and resolve triggers
- *(twitch)* Wire whisper-received trigger with text filters
- *(twitch)* Wire guest-star slot-updated trigger
- *(twitch)* Wire warning-sent trigger
- *(twitch)* Wire user-update account trigger

### 🚜 Refactor
- *(ui)* Fold twitch reauth into panel message

### 🧪 Testing
- *(twitch)* Cover Helix transport seam, chat send, and dispatch table
- *(twitch)* Add regression for multibyte message length limit
- *(twitch)* Cover announcement runner execution, limits, and registration
- *(twitch)* Cover cheer and shared-chat triggers with payload surfacing
- *(twitch)* Cover moderation runners request shape and mode matrix
- *(twitch)* Cover user-moderation runners resolve-then-act flow
- *(twitch)* Cover moderation-roles runners and shield mode
- *(twitch)* Cover reply-chat and send-whisper runners
- *(i18n)* Add regression for fmt_feed_time_pattern literal resolution
- *(twitch)* Cover channel-info and stream-marker runners
- *(twitch)* Cover shoutout and raid sub-action runners
- *(twitch)* Cover run-ad body shape and snooze-ad runners
- *(twitch)* Cover create-reward runner body mapping and validation
- *(twitch)* Cover update-reward partial-update runner
- *(twitch)* Cover reward enable/disable/pause/resume runners
- *(twitch)* Cover delete-reward and redemption-status runners
- *(ratelimit)* Cover token-bucket grant, throttle, and cooldown
- *(twitch)* Cover helix throttle loop and 429 backoff feed
- *(twitch)* Cover poll start and end runners
- *(twitch)* Cover prediction.start runner and validation
- *(twitch)* Cover prediction lock/cancel/resolve runners
- *(twitch)* Cover get-current-goal outputs and is_achieved derivation
- *(twitch)* Cover automod approve and deny message runners
- *(twitch)* Cover automod update-settings merge and overall modes
- *(twitch)* Cover add/remove blocked-term runners
- *(twitch)* Cover guest-star invite/assign-slot/remove runners
- *(twitch)* Cover guest-star update-slot and end-session runners
- *(twitch)* Cover channel-points redemption trigger filter and publish
- *(twitch)* Cover chat message-deleted and cleared triggers
- *(twitch)* Cover hype train begin/progress/end triggers
- *(twitch)* Cover charity donation filter and campaign lifecycle publish
- *(twitch)* Cover moderator add/remove and shield mode triggers
- *(twitch)* Cover shoutout, suspicious-user, and warning triggers
- *(twitch)* Cover poll begin/progress/end triggers and publishers
- *(twitch)* Cover prediction begin/progress/lock/end triggers
- *(twitch)* Cover goal begin/progress/end triggers
- *(twitch)* Cover reward CRUD and redemption status-filter triggers
- *(twitch)* Cover automod-hold and chat-settings triggers
- *(twitch)* Cover guest-star session and settings triggers
- *(twitch)* Cover automod settings/terms/message-updated triggers
- *(twitch)* Cover shared-chat session trigger arg-stacks and publishers
- *(twitch)* Cover channel-update, ad-break, auto-reward triggers
- *(twitch)* Cover guest-star guest-updated trigger and publish
- *(actions)* Cover generic sub-action form round-trip
- *(twitch)* Cover vip add/remove arg-stack and dispatch routing
- *(twitch)* Cover unban-request arg stacks and dispatch routing
- *(twitch)* Cover whisper-received trigger filter logic
- *(twitch)* Cover guest-star slot-updated arg stack
- *(twitch)* Cover warning-sent arg-stack and chat-rules marshaling
- *(twitch)* Cover user-update arg stack and PII guard

## [0.1.0-beta.11] - 2026-06-11
### ⚠️ BREAKING CHANGES
- **trovo**: EventSource/PlatformId/ChatSource lose Trovo variant

### ⚙️ Miscellaneous Tasks
- *(workspace)* Drop tautological and useless tests
- *(trovo)* [**breaking**] Remove integration after upstream decommissions streaming
- *(workspace)* Sync lockfile with widgets dev-dependency
- Release

### 🐛 Bug Fixes
- *(storage)* Drop invalid action_history filter from trovo migration
- *(vtube)* Supervisor starts lazily; sub-actions via switchable sink
- *(hotkey)* Read evdev via AsyncFd so exit is not blocked
- *(app)* Shut down integrations and sqlite pool on window close
- *(app)* Nest lifecycle messages under one Message variant
- *(app)* Localize missed quick-actions, console and audio labels
- *(vtube)* Recover switchable sink locks from poisoned state
- *(obs)* Register sub-actions via switchable sink for lazy boot
- *(widgets)* Keep tr-miss key referenced in release builds
- *(discord)* Guard health age-out against Instant underflow
- *(vtube)* Guard health age-out against Instant underflow

### 📚 Documentation
- Refresh README audit and document
- Restore OBS action coverage in README feature list
- Fix install artifacts, restore badges, drop onboarding claim
- *(release)* Release v0.1.0-beta.11

### 🚀 Features
- *(storage)* Add Language enum with persistence
- *(widgets)* Add fluent-rs i18n foundation + tr! macro
- *(app)* Add language picker to Settings → Language pane
- *(app)* Localize navigation and home screens via Fluent keys
- *(app)* Localize settings screens via Fluent keys
- *(app)* Localize actions and triggers screens via Fluent keys
- *(app)* Localize TTS, soundboard and queue screens via Fluent keys
- *(app)* Localize platform and integration screens via Fluent keys
- *(app)* Localize chat, feed, globals and script screens
- *(widgets)* Localize widget prose via Fluent keys
- *(widgets)* Add locale-aware date, number and time formatting
- *(storage)* Add Density enum with persistence
- *(app)* Add UI density picker with live spacing scaling
- *(storage)* Add font override settings accessors
- *(app)* Add interface and monospace font pickers
- *(app)* Add rebindable in-app keyboard shortcuts editor

### 🧪 Testing
- *(storage)* Cover Language enum contract
- *(i18n)* Verify locale key parity, fallbacks and uk plurals
- *(widgets)* Cover locale formatting, density tokens and chords
- *(app)* Cover shortcuts state machine and font preference checks
- *(storage)* Cover density, font accessors and pool shutdown
- *(vtube)* Empty switchable sink rejects calls as NotConnected
- *(obs)* Empty switchable sink rejects calls as Disconnected

## [0.1.0-beta.10] - 2026-06-08
### ⚙️ Miscellaneous Tasks
- Release

### 🚀 Features
- *(storage)* Add transit types for bundle import/export
- *(storage)* Add BundleRepo trait with import/export API
- *(storage-sqlite)* Implement BundleRepo import and export logic

### 🛠️ Build
- *(cache)* Scope save-if to main + drop cache-on-failure

### 🧪 Testing
- *(hotkey)* Fix canonical order in cmdorctrl macos test

## [0.1.0-beta.9] - 2026-06-08
### ⚙️ Miscellaneous Tasks
- Release

### 🎨 Styling
- *(runtime)* Apply rustfmt to script_run_* wrappers
- *(script)* Compress tautological doc comments

### 📚 Documentation
- *(release)* Release v0.1.0-beta.9

### 🚀 Features
- *(script)* Add ForgeApi method catalog + SymbolKind
- *(script)* Add ScriptHttpConfig + http_* settings keys
- *(script)* Add IP deny-list + is_private_or_special helper
- *(widgets)* Add TagListInput<Msg> for chip-style allowlist editing
- *(widgets)* Add ScriptEditorOverlay positioning shell
- *(widgets)* Add Esc + click-outside dismiss to ScriptEditorOverlay
- *(widgets)* Add autocomplete_popup widget, filter, and kind badges
- *(widgets)* Add hover_popover widget + signature formatter
- *(widgets)* Add ScriptEditorWidget composing rhai_editor + overlays
- *(app)* Adopt ScriptEditorWidget in script_editor screen
- *(app)* Add Settings → Scripting sub-screen
- *(app)* Add ScriptEditor toolbar (Debug, Format, API docs)
- *(app)* Add ScriptEditor type-check, Rhai version, Ln/Col pills
- *(app)* Add ScriptEditor run-stats line in console
- *(script)* Add ScriptHttpClient with sandbox validation pipeline
- *(script)* Register forge::http::get and post rhai bindings
- *(runtime)* Wire ScriptHttpClient into RunScript runner
- *(widgets)* Add autocomplete trigger predicate (. :: Ctrl+Space)
- *(script)* Add user-function hover from @input/@return docs

### 🧪 Testing
- *(script)* Rename version-anchored catalog test
- *(widgets)* Add autocomplete pipeline performance budget

## [0.1.0-beta.8] - 2026-06-07
### ⚙️ Miscellaneous Tasks
- Release

### 🎨 Styling
- *(tts-sapi)* Collapse com_stream let-binding for nightly fmt
- *(script)* Condense multi-paragraph docs to single-line invariants

### 📚 Documentation
- *(release)* Release v0.1.0-beta.8

### 🚀 Features
- *(types)* Add AnnotationDiagnostic for script editor surface
- *(script)* Add collect_annotation_diagnostics stub
- *(widgets)* Add RhaiTokenKind enum for syntax highlighter
- *(widgets)* Add tokenize_line rhai lexer
- *(widgets)* Impl iced Highlighter for RhaiHighlighter and wire into rhai_editor
- *(app)* Wire annotation_diagnostics + diagnostic status bar
- *(widgets)* LoomApi signature registry + status-bar type hints
- *(script)* Impl collect_annotation_diagnostics for doc comments

### 🚜 Refactor
- *(widgets)* Rename code_editor to rhai_editor + add RhaiHighlighterSettings

### 🛠️ Build
- *(release)* Link to .sha256 sidecar instead of inline hash

### 🧪 Testing
- *(widgets)* Add rhai lexer correctness fixtures
- *(widgets)* Add highlighter 500-line performance test

## [0.1.0-beta.7] - 2026-06-06
### ⚙️ Miscellaneous Tasks
- *(discord)* Remove unused webhook credential loader/writer
- *(workspace)* Update Cargo.lock for midir workspace dep
- *(workspace)* Update Cargo.lock after MIDI wiring
- *(workspace)* Update Cargo.lock after Hotkey wiring
- Release
- *(deps)* Bump windows from 0.58.0 to 0.62.2 (#22)
- *(deps)* Bump twitch_api from 0.7.2 to 0.8.0 (#23)
- *(deps)* Bump rhai from 1.25.0 to 1.25.1 (#24)
- *(deps)* Bump which from 4.4.2 to 8.0.2 (#25)
- *(deps)* Bump mockall from 0.13.1 to 0.14.0 (#26)
- Migrate version to workspace and apply formatter
- Added assets
- *(deps)* Bump midir/global-hotkey/zbus + adapt cpal 0.18 api

### 🐛 Bug Fixes
- *(discord)* Strip webhook URL from reqwest error messages
- *(discord)* Panic on reqwest client TLS init failure
- *(app)* Handle PickerKind::MidiPort in builtin_detail match arms
- *(app)* Handle missing existing hotkey id in conflict modal
- *(twitch)* Redact bearer token in StoredCredential Debug
- *(twitch)* Strip URLs from reqwest error display
- *(twitch)* Sanitize HelixRequestError display for token leaks
- *(hotkey)* Resolve combo via id lookup in global backend
- *(hotkey)* Gate emit_portal_unavailable to linux
- *(app)* Add description field for cargo-deb packaging
- *(hotkey)* Manually impl send+sync for global backend
- *(tts-sapi)* Import BOOL from windows::core for 0.62
- *(tts-sapi)* Pass rust bool to win32 fns for windows 0.62

### 📚 Documentation
- *(release)* Release v0.1.0-beta.7

### 🚀 Features
- *(discord)* Scaffold forge-discord crate skeleton
- *(discord)* Add DiscordError + WebhookCredential with redacted Debug
- *(discord)* Add DiscordEmbed, DiscordConfig, DiscordClient stub, sink + runners
- *(discord)* Add DiscordRateLimiter with global rate-limit tracking
- *(discord)* Implement embed control-character validation
- *(discord)* Implement webhook HTTP post with rate-limit + retry
- *(discord)* Impl BuiltinStatus + Health + Content + QuickActions
- *(app)* Add discord_client field to RuntimeView
- *(app)* Add Discord boot handler and DiscordClientRef in message
- *(app)* Register Discord sub-actions at startup
- *(midi)* Scaffold forge-midi crate skeleton
- *(midi)* Add MidiError, MidiEvent, MidiOutMessage, MidiPortInfo types
- *(midi)* Add MidiBackend trait with MidirBackend impl and PickerKind::MidiPort
- *(midi)* Implement MIDI byte decoding with message_to_bytes
- *(midi)* Implement supervisor with hot-plug detection and event dispatch
- *(midi)* Implement BuiltinStatus/Health/Content/QuickActions and MidiSink
- *(midi)* Add MidiSend sub-action runner
- *(midi)* Add MidiNoteOn/NoteOff/Cc trigger descriptors
- *(app)* Add midi_client field to RuntimeView
- *(app)* Add MIDI boot handler and MidiClientRef in message
- *(app)* Register MIDI sub-actions and triggers at startup
- *(hotkey)* Scaffold crate with error, combo, and config
- *(hotkey)* Add HotkeyBackend trait with NullBackend and test mocks
- *(hotkey)* Implement PortalBackend with NameOwnerChanged recovery
- *(hotkey)* Implement EvdevBackend as Linux fallback
- *(hotkey)* Implement GlobalHotkeyBackend for Windows and macOS
- *(hotkey)* Implement HotkeyClient with supervisor and health state
- *(hotkey)* Add HotkeyPressed trigger and sub-action registration
- *(hotkey)* Implement BuiltinStatus, BuiltinContent, and QuickActions
- *(app)* Add hotkey_client field to RuntimeView
- *(app)* Add Hotkey boot handler and HotkeyClientRef in message
- *(app)* Register Hotkey triggers and spawn HotkeyClient at startup
- *(widgets)* Add KeyCapture widget for hotkey combo input
- *(app)* Add Settings → Hotkeys screen

### 🚜 Refactor
- *(midi)* Downgrade MidiBackend trait visibility

### 🛠️ Build
- *(release)* Replace cargo-dist with hand-rolled gui packaging matrix
- Optimize build workflow and pass secrets
- *(release)* Install rust toolchain from rust-toolchain.toml

### 🧪 Testing
- *(discord)* Add wiremock delivery, rate-limit, and embed HTTP tests
- *(discord)* Assert webhook URL never appears in error display

## [0.1.0-beta.6] - 2026-06-04
### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(vtube)* Skip move_model call when all positional coords are None

### 🚀 Features
- *(vtube)* Scaffold forge-vtube crate with error type
- *(vtube)* Add credentials helpers and auth state machine
- *(vtube)* Add VTubeClient with WS connect and envelope codec
- *(vtube)* Add backoff reconnect supervisor with health events
- *(vtube)* Wire AuthenticationTokenRequest and AuthenticationRequest
- *(vtube)* Subscribe model+hotkey+tracking events and emit bus events
- *(vtube)* Impl BuiltinHealth with model / tracking / fps / api metrics
- *(vtube)* Impl BuiltinContent with models, hotkeys, expressions
- *(vtube)* Impl QuickActions (hotkey / expression / model / move)
- *(vtube)* Add hotkey_trigger sub-action runner
- *(vtube)* Add expression_set sub-action runner
- *(vtube)* Add param_set sub-action runner
- *(vtube)* Add model_load sub-action runner
- *(vtube)* Add params_reset sub-action runner
- *(vtube)* Add model_move sub-action runner
- *(vtube)* Register all six runners via register_vtube_sub_actions
- *(app)* Add vtube_client field and boot path to RuntimeView
- *(app)* Connect VTube card to live connection state in stream_apps
- *(app)* Register VTube sub-actions at startup

### 🚜 Refactor
- *(vtube)* Extract BuiltinStatus impl into status.rs
- *(vtube)* Extract supervisor loop into supervisor.rs

## [0.1.0-beta.5] - 2026-06-03
### ⚠️ BREAKING CHANGES
- **tts-cloud**: PollyCredentials gains base_url field (serde default = None)

### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock for forge-tts-cloud
- Release

### 🎨 Styling
- Drop tautological docstrings on TTS core and policy
- Drop repeated Polly 403 comment from match arms and tests

### 🐛 Bug Fixes
- *(tts-cloud)* Redact secrets in credential Debug output
- *(app)* Replace zero-size text placeholders with Space widget
- *(tts-nsspeech)* Unblock macOS build
- *(tts-sapi)* Rewrite against windows-rs 0.58 to unblock build
- *(tts-piper)* Make voices_dir_path test platform-agnostic

### 📚 Documentation
- *(release)* Release v0.1.0-beta.5

### 🚀 Features
- *(tts-core)* Add TtsError::QuotaExceeded variant
- *(tts-core)* Add TtsEngine::test_connection default impl
- *(tts-cloud)* Scaffold feature-gated crate with 4 engine stubs
- *(tts-cloud)* Add retry/timeout/rate-limit policy module
- *(audio)* Add decode_bytes for in-memory audio decoding
- *(tts-cloud)* Implement OpenAI synthesize via retry helper
- *(tts-cloud)* Implement ElevenLabs synthesize via retry helper
- *(tts-cloud)* Fetch ElevenLabs voices from /v1/voices endpoint
- *(tts-cloud)* Implement Azure Speech Service TTS engine
- *(tts-cloud)* [**breaking**] Add Polly SigV4 signer and error types
- *(tts-cloud)* Implement Polly synthesize via retry helper
- *(tts-cloud)* Fetch Polly voices and wire engine with retry
- *(app)* Load cloud TTS credentials and register engines at boot
- *(app)* Add Cloud TTS Engines screen with credentials form

### 🚜 Refactor
- *(speak-queue)* Hold TtsRegistry behind Arc<RwLock>
- *(tts-cloud)* Make EngineFactory credentials field private

### 🧪 Testing
- *(tts)* Remove real-service tests from nsspeech and sapi

## [0.1.0-beta.4] - 2026-06-01
### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock for forge-tts-espeak
- Release

### 🎨 Styling
- Drop tautological rustdoc and stale comments

### 🐛 Bug Fixes
- *(tts-piper)* Gate voice_scan tests under cfg(unix)

### 📚 Documentation
- *(release)* Release v0.1.0-beta.4

### 🚀 Features
- *(tts-espeak)* Scaffold crate with subprocess engine factory
- *(tts-espeak)* Parse espeak-ng --voices into TtsVoice catalog
- *(tts-espeak)* Synthesize via subprocess pipe to PcmBuffer
- *(app)* Register eSpeak-NG engine in TTS engine registry
- *(tts-sapi)* Scaffold crate with stub for non-Windows
- *(tts-sapi)* Add LCID-to-BCP47 conversion and voice metadata helpers
- *(tts-sapi)* Map SynthesisRequest to SAPI rate and SSML pitch
- *(tts-sapi)* Add STA worker thread and ISpVoice lifecycle
- *(app)* Register SAPI engine in TTS engine registry
- *(tts-nsspeech)* Scaffold crate with stub for non-macOS
- *(tts-nsspeech)* Map SynthesisRequest to AVSpeech rate + pitch
- *(tts-nsspeech)* Add AVFoundation worker and synthesis path
- *(app)* Register AVFoundation TTS engine in TTS registry
- *(app)* Show registered TTS engines in Tts → Engines screen

## [0.1.0-beta.3] - 2026-06-01
### ⚙️ Miscellaneous Tasks
- *(workspace)* Regenerate Cargo.lock for forge-platform-kick
- Release

### 📚 Documentation
- *(release)* Release v0.1.0-beta.3

### 🚀 Features
- *(platforms)* Add limited_reason field to PlatformCapabilities
- *(kick)* Scaffold crate with AuthFlow::None and capabilities
- *(runtime)* Register Kick triggers + mark platform Available
- *(kick)* Switch AuthFlow to LocalCallback with OAuth PKCE
- *(kick)* Add OAuth chat-send with credential refresh manager
- *(app)* Wire Kick OAuth through LocalCallbackFlow
- *(app)* Wire Kick boot chat read + OAuth send bridge

## [0.1.0-beta.2] - 2026-05-31
### ⚙️ Miscellaneous Tasks
- *(workspace)* Bump toolchain to 1.96.0
- Remove docs/ — README and in-app docs are canonical
- *(workspace)* Regenerate Cargo.lock for forge-platform-trovo
- Release

### 🐛 Bug Fixes
- *(livechat)* Use platform-specific palette colors for chips
- *(platforms)* Drop coming-soon stubs for YouTube and Trovo
- *(oauth)* Wire Trovo + YouTube Connect through LocalCallbackFlow
- *(app)* Show Platforms/Stream apps parent in builtin breadcrumb
- *(app)* Add Automation/Builtin parents to remaining breadcrumbs

### 📚 Documentation
- *(release)* Release v0.1.0-beta.2

### 🚀 Features
- *(oauth)* Add LocalCallbackDriver with PKCE and CSRF state
- *(twitch)* Switch to OAuth Authorization Code + PKCE
- *(youtube)* Switch to OAuth Authorization Code + PKCE
- *(trovo)* Scaffold crate with AuthFlow factory
- *(trovo)* Implement OAuth Authorization Code flow
- *(trovo)* Add credentials manager with token refresh
- *(trovo)* Add chat WebSocket subscriber
- *(trovo)* Add send-chat action
- *(trovo)* Register chat/spell/gift/follow triggers
- *(runtime)* Wire Trovo credentials + chat into runtime

### 🚜 Refactor
- *(app)* Rename DeviceCodeFlow screen to LocalCallbackFlow
- *(app)* Merge LocalCallbackFlow screen into BuiltinDetail

## [0.1.0-beta.1] - 2026-05-31
### ⚙️ Miscellaneous Tasks
- *(workspace)* Regenerate Cargo.lock for forge-platform-youtube
- Release

### 🐛 Bug Fixes
- *(platform-core)* Gate APP_DIR_NAME on non-macos targets
- Track new modules from earlier SRP refactor commits

### 📚 Documentation
- Add YouTube platform guide + README beta-1 mention
- *(release)* Release v0.1.0-beta.1

### 🚀 Features
- *(events)* Add EventSource::to_platform_id helper
- *(widgets)* Add platform color tokens to ForgePalette
- *(types)* Add PlatformScope and trigger_instances migration
- *(platform-youtube)* Scaffold crate with DeviceCode AuthFlow
- *(registry)* Add KindPlatformContract to descriptor trait
- *(runtime)* Add platform scope guard to TriggerEvaluator
- *(types)* Add UnifiedChatRow and supporting types
- *(runtime)* Add ChatPayload envelope and chat_stream bus bridge
- *(platform-youtube)* Implement GoogleAuthFlow device-code polling
- *(platform-twitch)* Emit _chat payload on chat and events
- *(app)* Add platform-scope picker to trigger editor
- *(platform-youtube)* Add credentials manager with on-demand refresh
- *(platform-youtube)* Add live chat polling with quota guard
- *(platform-youtube)* Register 10 YouTube trigger descriptors
- *(app)* Add DeviceCodeFlow screen for OAuth completion
- *(app)* Refactor LiveChat to render UnifiedChatRow
- *(runtime)* Wire YouTube credentials + chat poller + triggers
- *(app)* Wire Settings platforms to DeviceCodeFlow screen
- *(platform-youtube)* Implement send-chat with liveChatId tracking

### 🚜 Refactor
- *(types)* Promote PlatformId to forge-types
- *(platform-youtube)* Split chat_poller per SRP
- *(app)* Extract LiveChat view into separate module

### 🛠️ Build
- Pass YouTube OAuth credentials to CI builds

## [0.1.0-alpha.16] - 2026-05-29
### ⚠️ BREAKING CHANGES
- **general**: DataProvider::command_repo() removed;
Command/CommandId/CommandPermission/CommandRepo/CommandParser deleted;
migration 0013 drops the commands table

### ⚙️ Miscellaneous Tasks
- *(workspace)* Remove tautological comments
- Release

### 🐛 Bug Fixes
- *(ci)* Install libasound2-dev on Linux for cpal/alsa-sys builds
- *(storage)* Enable foreign_keys pragma on SqliteConnectOptions
- *(app)* Render trigger picker overlay on Actions list page
- *(app)* Make trigger picker assign refresh detail and surface custom vs default
- *(ui)* Click on trigger now moves you to editor

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.16

### 🚀 Features
- *(storage)* Add migration 0012 for trigger_instances and action_trigger_instances tables
- *(types)* Add TriggerInstance and TriggerInstanceId types
- *(storage)* Add TriggerInstanceRepo trait and ReferenceBlock error variant
- *(storage)* Implement SqliteTriggerInstanceRepo and wire DataProvider accessor
- *(registry)* Add effective_config merge helper for Template/Patch model
- *(app)* Wire TriggerRegistry into boot with default-instance upsert
- *(runtime)* Switch TriggerEvaluator to TriggerInstance with effective_config merge
- *(runtime)* Apply effective_config to sub-action steps before execute
- *(app)* Scaffold Triggers Registry screen with empty state
- *(storage)* Add list_all method to TriggerInstanceRepo
- *(app)* Implement Triggers Registry list, filters, side-sheet, and confirm-disable dialog
- *(app)* Remove legacy Commands UI surface
- *(storage)* Add link_action and unlink_action to TriggerInstanceRepo
- *(app)* Migrate ActionDetail from Trigger to TriggerInstance and rewire add-trigger flow
- *(app)* Replace add-trigger modal with 3-Level Picker for Actions side-sheet
- [**breaking**] Remove legacy Commands subsystem across all layers
- *(storage)* Remove legacy Trigger struct and TriggerRepo across all layers
- *(app)* Wire cross-navigation between Actions and Triggers Registry
- *(app)* Add Create Trigger Instance form on Triggers Registry page

### 🧪 Testing
- *(runtime)* Add e2e smoke for Template/Patch trigger and sub-action effective_config

## [0.1.0-alpha.15] - 2026-05-27
### ⚙️ Miscellaneous Tasks
- *(deps)* Bump actions/download-artifact from 7 to 8 (#16)
- *(deps)* Bump actions/upload-artifact from 6 to 7 (#17)
- *(deps)* Bump serde_json from 1.0.149 to 1.0.150 (#18)
- *(deps)* Bump tower-http from 0.6.10 to 0.6.11 (#19)
- *(deps)* Bump sqlx from 0.8.6 to 0.9.0 (#20)
- *(deps)* Bump rhai from 1.24.0 to 1.25.0 (#21)
- Release

### 🐛 Bug Fixes
- *(widgets)* Batch SideSheet layout invalidation
- *(widgets)* Render menu_button panel as overlay, not inline column
- *(app)* Remove redundant menu dismiss wrapper around drawer panel
- *(storage-sqlite)* Adapt query_as calls to sqlx 0.9 SqlSafeStr
- *(twitch)* Migrate builtin quick action templates to SubActionStep
- *(widgets)* Migrate quick action test fixtures to SubActionStep
- *(server)* Migrate test fixtures and spawn_action_engine to registry-based dispatch
- *(storage)* Migrate trigger and action tests to kind_id and SubActionStep

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.15

### 🚀 Features
- *(widgets)* Add SideSheet widget skeleton with overlay layout
- *(storage)* Add sheet_width typed accessors to SettingsRepo
- *(widgets)* Animate SideSheet open/close with eased x-offset
- *(widgets)* Add drag-to-resize handle to SideSheet
- *(registry)* Scaffold forge-registry crate with descriptor and runner traits
- *(twitch)* Implement TriggerKindDescriptor for 7 trigger kinds
- *(audio)* Implement SubActionRunner for soundboard.sound.play and tts.speak.text
- *(obs)* Implement TriggerKindDescriptor and 8 SubActionRunner kinds
- *(runtime)* Migrate sub-actions and triggers to registry-based dispatch
- *(storage)* Migrate trigger and sub-action rows from enum JSON to kind_id format

### 🚜 Refactor
- *(app)* Extract home view fns from app.rs to home.rs
- *(app)* Extract settings view fns to settings.rs
- *(app)* Extract platforms overview view to platforms_view.rs
- *(app)* Extract actions view fns to actions_view.rs
- *(app)* Extract action modal views to actions_modals.rs
- *(app)* Extract navigation helpers to navigation.rs
- *(app)* Extract subscription wiring to subscriptions.rs
- *(app)* Extract tts section view fns to tts_view.rs
- *(app)* Extract boot async helpers to boot.rs
- *(app)* Extract view screen-dispatch router to view_router.rs
- *(app)* Extract page-chrome helpers to page_chrome module
- *(app)* Move boot/server result handlers to boot.rs
- *(app)* Extract Settings message handler to settings.rs
- *(app)* Extract navigate arm to navigation::handle_navigate
- *(app)* Extract viewer-drawer panel to live_chat_drawer.rs
- *(app)* Migrate viewer drawer to SideSheet widget
- *(widgets)* Drop legacy side_sheet fn and sheet_chrome chrome
- *(app)* Split actions.rs into trigger_kinds/forms/telemetry
- *(app)* Split action_editor.rs into view and update layers
- *(widgets)* Split builtin into header/health/quick_actions
- *(app)* Extract globals_variant_editor module from globals_view
- *(actions)* Extract ActionsService; UI dispatches via Tasks
- *(tests)* Introduce mockall
- *(storage)* DataProvider accessors return Arc<dyn XRepo>
- *(app)* Narrow DataProvider args to specific repos
- *(twitch)* Move credential format from forge-app
- *(runtime)* Extract dashboard::compute_stats from forge-app
- *(twitch)* Move EventSub parsers from forge-app
- *(script)* Move script execution from forge-app to forge-script
- *(app)* Nest boot/subsystem variants under Message envelopes
- *(widgets)* Split server.rs into per-widget files
- *(server)* Split protocol/mod.rs into 5 function families
- *(storage)* Split SettingsRepo into KV + typed configs
- *(runtime)* Narrow command-parser and trigger-evaluator services to per-repo Arcs
- *(server)* Narrow DispatchContext to 4 repo Arcs
- *(runtime)* Narrow 7 sub-action handlers from DataProvider to GlobalsRepo
- *(script)* Narrow ForgeApi and run_inline to Arc<dyn GlobalsRepo>
- *(runtime)* Narrow ActionEngine to ActionRepo+HistoryRepo+GlobalsRepo
- *(runtime)* Narrow ActionsService to per-repo Arcs
- *(server)* Narrow AppState and ServerConfig to 4 repo Arcs
- *(app)* Narrow boot to Arc<dyn DataProvider>, drop SqliteBackend _arc helpers
- *(types)* Swap TriggerKind/SubActionSpec enums for kind_id + SubActionStep
- *(ui)* Migrate forge-app to kind_id strings and SubActionStep
- *(obs)* Extract store_and_connect orchestration to forge-obs::credentials
- *(app)* Extract twitch reauth handler from central update

## [0.1.0-alpha.14] - 2026-05-24
### ⚙️ Miscellaneous Tasks
- Release

### ⚡ Performance
- *(chat)* Cache rendered rows with iced::widget::lazy

### 🎨 Styling
- *(widgets)* Bump inline pill padding from Xxs to Xs

### 🐛 Bug Fixes
- *(widgets)* Request redraw on chat username hover state change

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.14

### 🚀 Features
- *(widgets)* Add ChatRowWidget skeleton implementing iced Widget trait
- *(widgets)* Implement ChatRowWidget layout, draw, and update
- *(widgets)* Add dotted underline on chat username hover

### 🚜 Refactor
- *(app)* Introduce RuntimeView and move services to App::rt
- *(app)* Wrap EventArrived payload in Arc
- *(globals)* Migrate globals_view to RuntimeView contract
- *(queues)* Migrate queues_view to RuntimeView contract
- *(feed)* Migrate event_feed to RuntimeView contract
- *(commands)* Migrate commands_view to RuntimeView contract
- *(soundboard)* Migrate soundboard to RuntimeView contract
- *(server)* Migrate server_screen to RuntimeView contract
- *(settings)* Migrate settings_audio to RuntimeView contract
- *(settings)* Migrate settings_websocket to RuntimeView contract
- *(home)* Migrate home to RuntimeView contract
- *(tts-view)* Migrate tts_dashboard to RuntimeView contract
- *(tts-view)* Migrate tts_engines to RuntimeView contract
- *(tts-view)* Migrate tts_filters to RuntimeView contract
- *(tts-view)* Migrate tts_triggers to RuntimeView contract
- *(tts-view)* Migrate voice_aliases to RuntimeView contract
- *(editor)* Migrate script_editor to RuntimeView contract
- *(viewers)* Migrate viewers to RuntimeView contract
- *(workspace)* Rename Integration domain to Builtin
- *(builtin)* Migrate builtin_detail to RuntimeView contract
- *(chat)* Migrate live_chat to RuntimeView contract
- *(actions)* Migrate actions list handler to RuntimeView contract
- *(actions)* Migrate add_action handler into action_editor
- *(actions)* Migrate add_trigger handler to action_editor
- *(actions)* Migrate three sub-action handlers to action_editor
- *(obs)* Move obs_panel handler into module
- *(twitch)* Move twitch_panel handler into module
- *(tts-view)* Inline TTS sub-router into central match
- *(app)* Fan out EventArrived through per-screen on_event
- *(actions)* Nest 5 editor message variants under Actions::Editor
- *(app)* Group per-screen states under app ui struct
- *(app)* Depend on DataProvider trait instead of SqliteBackend
- *(chat)* Publish chat.send.request to bus instead of inline send
- *(widgets)* Convert chat_row API to owned data
- *(widgets)* Apply Spacing tokens to internal paddings
- *(app)* Apply Spacing tokens across screen paddings
- *(widgets)* Switch to ChatRowWidget, delete old chat_row API

## [0.1.0-alpha.13] - 2026-05-22
### ⚙️ Miscellaneous Tasks
- *(widgets)* Drop _v2 naming and remove dead sidebar code
- *(app)* Drop standalone Viewers screen now folded into chat drawer
- *(widgets)* Drop onboarding module and move banner to sections
- *(app)* Drop Settings::Platforms subsection (duplicates top-level)
- Release

### 🎨 Styling
- *(chat)* Highlight usernames on mouse hover
- *(widgets)* Input bar icons and styling
- *(widgets)* Use 4-step font sizes
- *(app)* Rebuild Stream apps overview
- *(app)* Rewrite TTS preview as per-stage cards
- *(actions)* Update actions styling
- *(ui)* Align border/radius tokens and integrate live home screen telemetry

### 🐛 Bug Fixes
- *(widgets)* Vertically center title bar content
- *(widgets)* Vertically center app footer content
- *(widgets)* Anchor sidebar Settings to bottom of viewport
- *(chat)* Use 2-row layout with row separator between messages
- *(livechat)* Align viewer drawer with design and wire username clicks
- *(chat)* Widen viewers drawer to 360px
- *(chat)* Bind drawer detail to clicked viewer with chat fallback
- *(chat)* Pad drawer action buttons so icons clear border
- *(widgets)* Replace unicode glyphs with tabler icons
- *(app)* Replace unicode glyphs with tabler icons
- *(app)* Flatten Platforms and Stream apps in sidebar per design
- *(app)* Align telemetry grid box size with empty placeholders
- *(app)* Show only action count right-aligned in tree group header
- *(app)* Match trigger and sub-action row size with placeholder
- *(app)* Add inline add buttons to triggers and sub-actions headers
- *(app)* Use icon-prefixed ghost buttons for Test run and Duplicate
- *(chat)* Replace meta/filter bars with single page header
- *(home)* Replace global crumb_bar with screen page header

### 📚 Documentation
- *(widgets)* Strip tautological rustdoc from token definitions

### 🚀 Features
- *(widgets)* Add ModalSize design-token enum with 3 widths
- *(widgets)* Add semantic-color alpha helpers to palette
- *(widgets)* Add toast queue, viewport widget, and app wiring
- *(widgets)* Add popover MenuButton and RowActions widgets
- *(chat)* Rebuild layout and add 5 typed chat row renderers
- *(livechat)* Add viewers drawer panel with detail card and list
- *(widgets)* Add username click callback to chat row builders
- *(widgets)* Add hover affordance to chat username and ghost button
- *(widgets)* Switch search_input to Tabler icon and tighten padding
- *(widgets)* Add big_jump_card builder for Home dashboard
- *(app)* Rewrite Home screen and align font sizes to token scale
- *(storage)* Add action telemetry query and execution log
- *(app)* Add action telemetry grid, type filter chips, tree row hover
- *(app)* Add sub-action reorder controls and colored variable display
- *(widgets)* Add external-link icon to complex value preview
- *(app)* Add Platforms overview screen with 2x2 platform cards
- *(app)* Add generic platform/app placeholder for beta-N integrations
- *(app)* Add Commands screen and Settings Language/Shortcuts panes
- *(app)* Inline rename and verious styling
- *(widgets)* Add side_sheet overlay widget
- *(app)* Unify page headers across screens and migrate to side sheets
- *(chat)* Adds emoji and real viewer data

### 🚜 Refactor
- *(widgets)* Compress Spacing enum to 4 design-token variants
- *(widgets)* Compress font constants to 4 design tokens
- *(widgets)* Compress Radius enum to 3 design-token variants
- *(widgets)* Redesign title bar with logo, profile name, version
- *(app)* Reorganize sidebar with section labels per new design
- *(widgets)* Remove version label from title bar
- *(widgets)* Drop version and profile labels from title bar
- *(app)* Uppercase sidebar section labels at source
- *(widgets)* Migrate icons from Bootstrap font to Tabler SVG

## [0.1.0-alpha.12] - 2026-05-21
### ⚙️ Miscellaneous Tasks
- Release

### ⚡ Performance
- *(audio)* Cache cpal device enumeration with 5s TTL and explicit refresh

### 📚 Documentation
- *(readme)* Refresh current status for alpha-12 release
- *(release)* Release v0.1.0-alpha.12

### 🚀 Features
- *(scripts)* Add Variant::Datetime sub-actions ReadFile, RandomInt and forge::time rhai
- *(app)* Add ReadFile and RandomInt to Add SubAction picker
- *(app)* Add Viewers screen with ViewerRepo and platform tracking
- *(app)* Add Viewers screen Settings sub-screens and viewer tracker
- *(app)* Wire Hub hero uptime and title bar 8-subsystem counter
- *(app)* Wire Event Feed export and Actions duplicate button
- *(app)* Retro polish across actions twitch and event feed

### 🚜 Refactor
- *(app)* Rename Hub to Home across messages widgets and state

## [0.1.0-alpha.11] - 2026-05-21
### Deps
- *(libs)* Bump libraries versions

### ⚙️ Miscellaneous Tasks
- *(workspace)* Sync Cargo.lock for tts crate deps
- Release

### 🐛 Bug Fixes
- *(audio)* Adapt to rubato 3.0 and symphonia 0.6 APIs after bump
- *(app)* Register Piper engine and wire CpalSink in speak queue boot
- *(app)* Register Piper engine and wire CpalSink in speak queue boot
- *(ci)* Add libasound2-dev apt dep for cpal Linux build

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.11

### 🚀 Features
- *(tts-core)* Create forge-tts-core crate with TtsEngine trait
- *(voice)* Create forge-voice crate with VoiceAlias and resolver
- *(tts-pipeline)* Create forge-tts-pipeline crate skeleton
- *(tts-piper)* Create forge-tts-piper crate skeleton
- *(speak-queue)* Create forge-speak-queue crate skeleton
- *(storage)* Add VoiceAliasRepo trait and StoredVoiceAlias
- *(storage-sqlite)* Add migration 0007 voice_aliases and repo impl
- *(voice)* Implement VoiceAliasResolver with three strategies
- *(tts-pipeline)* Implement 5-stage process and preview API
- *(tts-piper)* Implement subprocess synthesis and voice scanner
- *(speak-queue)* Implement tokio actor with verbs and bus events
- *(storage)* Add TTS settings keys and pipeline config helpers
- *(types,runtime)* Add SubActionSpec::Speak and SpeakDispatcher
- *(speak-queue)* Expose subscribe() on SpeakQueueHandle
- *(app)* Add TtsSection enum and parameterize Screen::Tts
- *(tts-ui)* Add TTS message types and module declarations
- *(tts-ui)* Add TTS dashboard, engines, filters, triggers, voice aliases screens
- *(tts-ui)* Wire TTS state into App and add tts_section_view routing
- *(tts-ui)* Wire speak queue, SpeakBridge, and SpeakEvent subscription
- *(tts-ui)* Add Speak sub-action to action editor
- *(script)* Wire forge::tts rhai module via SpeakRequester trait

### 🧪 Testing
- *(voice)* Add resolver_deterministic regression tests
- *(voice)* Add resolver_blocked regression tests
- *(voice)* Add resolver_ignore_profile regression tests
- *(tts-pipeline)* Add pipeline_full_flow regression tests
- *(tts-pipeline)* Add pipeline_preview regression tests
- *(speak-queue)* Add queue_per_user_limit regression tests
- *(speak-queue)* Add queue_pause_resume regression tests
- *(speak-queue)* Add queue_priority regression tests
- *(tts-piper)* Add voice_scan regression tests
- *(runtime)* Add speak_dispatch regression tests

## [0.1.0-alpha.10] - 2026-05-21
### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock for new audio deps
- *(workspace)* Update Cargo.lock for forge-soundboard runtime dep
- *(workspace)* Sync Cargo.lock for forge-audio in forge-app deps
- Release

### 🎨 Styling
- *(tests)* Apply rustfmt to alpha-10 regression tests

### 🐛 Bug Fixes
- *(soundboard)* Emit PlaybackFailed on decode and sink-factory errors
- *(app)* Wire BusAudioEventSink so audio events reach the event bus

### 📚 Documentation
- *(readme)* Refresh current status for alpha-10 audio milestone

### 🚀 Features
- *(events)* Add Audio variant to EventSource enum
- *(types)* Add PlaySound SubAction variant with ClipId and OutputDevice
- *(audio)* Create forge-audio crate with AudioSink trait and cpal device discovery
- *(soundboard)* Create forge-soundboard crate with clip schema
- *(storage)* Add SoundboardClipsRepo trait and StoredClip
- *(storage-sqlite)* Add migration 0006 soundboard_clips and repo impl
- *(audio)* Add crossbeam-channel and futures to workspace deps
- *(audio)* Add AudioEvent enum and AudioEventSink trait
- *(audio)* Add convert module with rubato resampler and channel remix
- *(audio)* Add fan_out helper for multi-sink concurrent playback
- *(audio)* Add symphonia decoder for clip files
- *(audio)* Add CpalSink with bounded ring buffer playback
- *(soundboard)* Add AudioSinkFactory trait and CpalSinkFactory
- *(runtime)* Add SoundPlayer trait for soundboard integration
- *(soundboard)* Add BusAudioEventSink and SoundboardPlayer with tests
- *(runtime)* Wire SoundPlayer into PlaySound runner and spawn_action_engine
- *(widgets)* Add volume_slider widget
- *(widgets)* Add output_device_picker widget
- *(widgets)* Add clip_card widget
- *(app)* Add Soundboard screen with grid layout and add-clip modal
- *(app)* Add in-app hotkey subscription for Soundboard
- *(app)* Add Settings Audio sub-screen with device test-tone
- *(app)* Instantiate SoundboardPlayer at boot and wire to App
- *(app)* Wire Soundboard sidebar navigation and screen routing
- *(app)* Add PlaySound option to Add SubAction picker

### 🧪 Testing
- *(audio)* Add convert_correctness and decode_smoke integration tests
- *(soundboard)* Add player_emits_events and player_emits_failed regression tests
- *(runtime)* Add play_sound_dispatch integration tests via action engine

## [0.1.0-alpha.9] - 2026-05-20
### ⚠️ BREAKING CHANGES
- **runtime**: EventLogRepo gains recent_since() method
- **server**: ServerConfig::new now requires data_provider argument
- **server**: TriggerKind gains CodeEvent variant; exhaustive matches must handle it

### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(app)* Hold tokio runtime guard so background spawns don't panic
- *(app)* Handle tokio runtime init error without panicking
- *(app)* Make sidebar Actions Twitch and OBS items reach real screens
- *(app)* Wire device code copy button and verification URL open
- *(widgets)* Pad onboarding step pills so text doesn't crowd the edges
- *(platform-core)* Surface Twitch device-code response body in errors
- *(storage-sqlite)* Ignore missing migrations to survive binary downgrade
- *(app)* Tighten Home empty card height and add stat row padding
- *(app)* Even Home bottom cards width and zero-default At a glance counts
- *(app)* Always route sidebar Twitch and OBS to IntegrationDetail
- *(platform-core)* Accept Twitch token error format with message field
- *(platform-core)* Accept Twitch scope field as either string or array
- *(storage-sqlite)* Hex-encode digest by byte for sha2 0.11 compat
- *(workspace)* Enable rustls TLS feature on tokio-tungstenite for EventSub
- *(widgets)* Map kebab-case icon names to bundled bootstrap codepoints
- *(platform-twitch)* Align scopes and quick actions with Twitch design
- *(platform-twitch)* Stop chat reconnect loop when token scope is missing

### 📚 Documentation
- *(readme)* Refresh current status for alpha-9 server milestone
- *(release)* Release v0.1.0-alpha.9

### 🚀 Features
- *(server)* Add axum router skeleton with ws/api/overlays routes
- *(storage)* Add 4 server config settings keys
- *(runtime)* [**breaking**] Add EventBus::recent_since with DB fallback
- *(server)* Add bearer-token auth middleware for /api/v1
- *(server)* Add bus-adapter with per-client filter dispatch
- *(server)* Add WsClient state with per-client ev/s tracking
- *(widgets)* Add throughput_sparkline canvas widget
- *(widgets)* Add bearer_token_display widget with mask and reveal
- *(server)* [**breaking**] Add WS protocol envelope and method dispatcher base
- *(server)* Implement subscribe and unsubscribe WS methods
- *(widgets)* Add client_table_row with 3-state dot and chips
- *(widgets)* Add bind_address_card radio widget
- *(server)* Implement getInfo with clients and bandwidth telemetry
- *(server)* Implement getActions and doAction WS methods
- *(widgets)* Add type_to_confirm_modal blocking widget
- *(widgets)* Add overlay_file_list with sizes and browser URL
- *(server)* Implement getCommands, getGlobals, getGlobal, setGlobal
- *(server)* [**breaking**] Implement getUserGlobals and triggerCodeEvent methods
- *(app)* Add Server screen with stats clients and overlay list
- *(server)* Implement getEvents and replayEvent WS methods
- *(app)* Add Settings WebSocket sub-screen with LAN-bind modal
- *(server)* Implement WS authenticate first-frame method
- *(server)* Implement getActiveViewers WS method
- *(server)* Implement getOverlayFiles WS method with sandbox
- *(server)* Add HTTP REST mirror at /api/v1
- *(server)* Serve overlay files with sandbox and CORS toggle
- *(server)* Notify clients of dropped events on backpressure lag
- *(server)* Add ServerHandle stop and restart lifecycle
- *(server)* Guard against unsafe LAN bind without token or flag
- *(server)* Expose auth_state and bind_addr getters on ServerHandle
- *(app)* Add server_subsystem wrapping ServerHandle lifecycle
- *(app)* Autostart server from settings on app boot
- *(app)* Wire server screen restart stop and regenerate buttons
- *(app)* Auto-restart server when WebSocket settings change
- *(app)* Subscribe to live server metrics when Server screen visible
- *(app)* Write tracing logs to daily-rotated files alongside console
- *(widgets)* Bundle Inter and JetBrains Mono so UI looks the same everywhere
- *(app)* Redesign Home empty state with dimmed values and create CTAs
- *(app)* Add Twitch disconnected inline panel with device code mockup
- *(app)* Wire Twitch device code flow polling and credential storage
- *(app)* Reconnect Twitch chat from stored credentials on app boot
- *(app)* Show Twitch connected panel with login when chat session is live
- *(app)* Add OBS disconnected inline panel with form and test connect
- *(platform-twitch)* Impl 4 integration traits for full connected view
- *(app)* Redesign Actions list as grouped table with filters and stats
- *(app)* Add Queues management screen with cards and pause controls
- *(app)* Show re-auth banner when Twitch token scope is missing
- *(platform-twitch)* Subscribe to all EventSub topics with shared tracker
- *(storage)* Aggregate last_ran and runs_24h via HistoryRepo stats_summary
- *(app)* Add Action editor screen with 2-pane tree and sub-action flow
- *(app)* Seed Hub triggers_fired from 24h history and increment on action.done
- *(runtime)* Expose paused_queues query and feed Queues view live state
- *(app)* Treat Drain as pause plus queue.drain_requested bus event
- *(server)* Track http_requests and events_out counters in ServerInfo
- *(app)* Surface live WS client subscriptions in Server screen rows
- *(app)* Scan overlay root from disk and feed Server screen file list
- *(app)* Replace active bool with 3-state liveness on Server client rows
- *(app)* Load Settings WebSocket state from storage and apply overlay config to server
- *(app)* Add hover affordance to Server connected-clients rows
- *(app)* Add kick-to-disconnect hint to Server clients header

### 🚜 Refactor
- *(app)* Remove Onboarding screen and route boot directly to Home
- *(platform-twitch)* Replace custom DCF with twitch_api and twitch_oauth2
- *(widgets)* Rewrite IntegrationDetail layout to match HTML designs
- *(platform-core)* Remove dead custom OAuth code now handled by twitch_oauth2

## [0.1.0-alpha.8] - 2026-05-19
### ⚠️ BREAKING CHANGES
- **storage**: DataProvider gains event_log_repo() method
- **runtime**: EventBus::replay renamed to lookup; replay name reserved for replay_and_publish
- **runtime**: EventBus::new now requires Arc<dyn EventLogRepo>
- **runtime**: none

### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock
- *(deps)* Update lockfile for forge-widgets forge-events dep
- *(workspace)* Update Cargo.lock and rustfmt cleanup
- Release

### 🐛 Bug Fixes
- *(obs)* Align event payloads with RFC-031 and populate caused_by
- *(twitch)* Align observability events with RFC-031 audit
- *(runtime)* [**breaking**] Populate caused_by across full event causation chain
- *(twitch)* Use char-boundary truncation for body_snippet
- *(widgets)* Cap json_viewer recursion depth to prevent stack overflow

### 📚 Documentation
- *(readme)* Remove Twitch client_id manual-setup limitation
- *(readme)* Refresh current status for alpha-8 EventFeed milestone
- *(release)* Release v0.1.0-alpha.8

### 🚀 Features
- *(events)* Add Event::replay flag for replay_and_publish
- *(storage)* [**breaking**] Add EventLogRepo trait and DataProvider extension
- *(storage-sqlite)* Add migration 0005 and EventLogRepo impl
- *(storage)* Add event_log_retention_days settings key
- *(storage-sqlite)* Add background event_log retention pruning task
- *(widgets)* Add source_badge widget with EventSource colors
- *(runtime)* [**breaking**] Add EventBus::replay_and_publish with DB fallback
- *(runtime)* Add ring buffer flush task for event_log persistence
- *(widgets)* Add event_row_observability widget
- *(widgets)* Add causation_chip widget
- *(widgets)* Add json_viewer widget with syntax colors
- *(runtime)* Emit global.set/incr/del observability events per RFC-031
- *(widgets)* Add replay_button widget
- *(runtime)* Emit queue and cooldown observability events
- *(widgets)* Add event_inspector composite widget
- *(script)* Emit script.exec and script.error events per RFC-031
- *(app)* Add EventFeed screen with filters and replay flow
- *(app)* Wire real EventLogRepo into EventBus at boot
- *(app)* Rewire TestTrigger to use EventBus::replay_and_publish

### 🚜 Refactor
- *(runtime)* [**breaking**] Rename EventBus::replay to lookup

### 🧪 Testing
- *(runtime)* Add replay correctness regression tests

## [0.1.0-alpha.7] - 2026-05-19
### ⚠️ BREAKING CHANGES
- **platform-core**: IntegrationStatus requires capability_flags() and header_actions()
- **platform-core**: HealthMetric loses sublabel/color; HealthValue replaces; IntegrationHealth adds stream()
- **types**: ExecutionContext.trigger_event_id replaced by metadata: ExecutionMetadata
- **platform-core**: IntegrationCatalog removed; replaced by IntegrationContent + DetailSection enum (8 variants)
- **platform-core**: QuickAction.payload removed; enabled, subaction_template, picker added; PickerKind enum with Copy+Hash added
- **obs**: ObsClient::connect now requires Arc<dyn EventPublisher> parameter
- **runtime**: ActionEngine now requires Option<Arc<dyn ObsSink>> as fourth argument to spawn_action_engine
- **types**: TriggerKind gains ObsSceneChanged variant; exhaustive matches must add the arm

### ⚙️ Miscellaneous Tasks
- *(workspace)* Update Cargo.lock
- *(deps)* Add futures-core to forge-platform-core lockfile
- *(deps)* Update Cargo.lock for forge-events dep in forge-obs
- Release

### 🐛 Bug Fixes
- *(obs)* Align scene event kinds with EventSource::Obs taxonomy

### 📚 Documentation
- *(readme)* Add dynamic release and activity badge block
- *(readme)* Refresh current status for alpha-7 OBS milestone
- *(release)* Release v0.1.0-alpha.7

### 🚀 Features
- *(obs)* Add forge-obs crate skeleton
- *(platform-core)* [**breaking**] Extend IntegrationStatus with capability flags
- *(types)* Add OBS sub-action variants and runtime stubs
- *(platform-core)* [**breaking**] Replace IntegrationHealth metrics with stream shape
- *(types)* [**breaking**] Add ExecutionMetadata enum to ExecutionContext
- *(platform-core)* [**breaking**] Replace IntegrationCatalog with DetailSection
- *(platform-core)* [**breaking**] Rework QuickAction shape and add PickerKind
- *(obs)* Define ObsSink and ObsSource traits
- *(widgets)* Add integration_header_card widget
- *(obs)* Implement ObsClient connect with challenge-response auth
- *(widgets)* Add integration_health_grid widget
- *(obs)* Add reconnect with exponential backoff and jitter
- *(obs)* Impl IntegrationStatus for ObsClient
- *(obs)* Impl IntegrationHealth with stream-based deltas
- *(widgets)* Add integration_content_renderer dispatcher
- *(obs)* Impl IntegrationContent with scenes and sources
- *(obs)* Impl QuickActions with 4 actions and pickers
- *(widgets)* Add integration_quick_actions_grid widget
- *(obs)* Impl ObsSink methods via obws API calls
- *(app)* Add IntegrationDetail generic screen
- *(obs)* [**breaking**] Map obws events to forge bus EventSource::Obs
- *(app)* Add StreamApps landing screen with OBS card
- *(runtime)* [**breaking**] Wire OBS sub-action runners to ObsSink
- *(widgets)* Add full-screen picker_modal widget
- *(app)* Wire onboarding ConnectObs step to real flow
- *(types)* [**breaking**] Add Trigger::ObsSceneChanged variant and evaluator
- *(app)* Boot ObsClient and populate IntegrationDetail state
- *(app)* Wire QuickAction picker to SubAction enqueue and toast

### 🛠️ Build
- Remove redundant token-redaction job from pr and nightly
- *(manual)* Dedupe artifact upload via runner.os ternary
- *(nightly)* Use gh api for commit check instead of full checkout
- *(nightly)* Add explicit should_run guard to test job

### 🧪 Testing
- *(obs)* Add backoff monotonicity and reconnect-supervisor stub
- *(runtime)* Add OBS sub-action delegation regression with RecordingSink
- *(runtime)* Add QuickAction vs SubAction equivalence and event-kind regression

## [0.1.0-alpha.6] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **types**: VariantKind::color() removed — use forge_widgets::variant_kind_color(kind, palette) instead
- **storage**: ScriptRecord.source_code renamed to body; description removed; contract: ScriptContract and body_hash: String added; upsert renamed to save.
- **script**: Engine::with_config now stores wall_timer as field; Engine struct is no longer Send (was already not Send due to rhai::Engine).
- **runtime**: dispatch signature gains registry param; bus changed to &Arc<EventBus>, dp to Arc<dyn DataProvider>
- **runtime**: spawn_action_engine now requires Arc<ScriptRegistry>

### ⚙️ Miscellaneous Tasks
- *(release)* Release v0.1.0-alpha.6
- Release

### 🐛 Bug Fixes
- Remove stage-marker strings from docs and error messages
- *(script)* Use Engine::new_raw with explicit packages and sleep budget

### 🚀 Features
- *(storage)* [**breaking**] Add migration 0004 and ScriptContract to ScriptRepo
- *(script)* [**breaking**] Add ForgeApi god-object with sandbox limits
- *(script)* Add doc-comment contract parser and scope builder
- *(runtime)* Add ScriptRegistry with hot-reload
- *(runtime)* [**breaking**] Add RunScript sub-action runner with registry lookup
- *(widgets)* Add console widget with typed prefix levels
- *(widgets)* Add code_editor wrapper with line numbers
- *(app)* Build ScriptEditor 3-pane screen
- *(runtime)* [**breaking**] Wire ScriptRegistry into ActionEngine for RunScript

### 🚜 Refactor
- *(types)* [**breaking**] Move VariantKind to forge-types from forge-widgets

### 🛠️ Build
- *(release)* Allow dirty release.yml for intentional manual deltas

### 🧪 Testing
- *(runtime)* Add ignored regression for RunScript wiring gap in ActionEngine

## [0.1.0-alpha.5] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **runtime**: SubActionSpec gets 3 new variants — match arms in user code must be exhaustive
- **widgets**: forge-widgets now depends on forge-types

### ⚙️ Miscellaneous Tasks
- Release

### 🎨 Styling
- *(cliff)* Adds fix for typo in commit history

### 🚀 Features
- *(app)* Rebuild Hub screen per design mockup
- *(widgets)* Add title_bar_v2 breadcrumb with status pills
- *(widgets)* Add sidebar_v2 with nested groups and status dots
- *(app)* Rebuild Home content per v2 and wire chrome v2
- *(storage)* Wire globals reads/writes counters and add incr
- *(storage)* Add GlobalTransit, GlobalsExport and export_all
- *(runtime)* [**breaking**] Add globals sub-action runners and interpolation hook
- *(widgets)* [**breaking**] Add Tier 1.5 data widgets for globals editor
- *(app)* Build Globals screen with filter search and table
- *(app)* Add Variant editor modal for globals create and edit
- *(app)* Wire globals JSON export to native file dialog

### 🚜 Refactor
- *(app)* Rename Screen::Hub to Screen::Home

### 🧪 Testing
- *(runtime)* Assert interpolation increments globals reads counter

## [0.1.0-alpha.4] - 2026-05-18
### ⚠️ BREAKING CHANGES
- **paths**: data path on Windows now %APPDATA%\icesqueez\forge\data\, on macOS ~/Library/Application Support/com.icesqueez.forge/. Linux unchanged.
- **storage**: repo traits now return forge_types domain types; DataProvider gains 5 accessor methods replacing supertrait composition for those repos
- **twitch**: ChatSendBridge::spawn now takes Arc<dyn CredentialsRepo>

### ⚙️ Miscellaneous Tasks
- Release

### 🐛 Bug Fixes
- *(paths)* Use BaseDirs to drop owner segment from Windows path

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.4

### 🚀 Features
- *(paths)* [**breaking**] Add cross-platform ProjectDirs helper and migrate call-sites
- *(types)* Add Action Trigger Command Queue SubAction primitives
- *(storage)* [**breaking**] Replace ActionRepo/TriggerRepo/CommandRepo/QueueRepo/HistoryRepo with domain types
- *(runtime)* Add ActionEngine with sub-action dispatch loop
- *(runtime)* Add QueueScheduler with pause and concurrency control
- *(runtime)* Add SendChat Delay SetGlobal sub-action runners
- *(runtime)* Add CommandParser for chat-message dispatch
- *(widgets)* Add actions editor widget family
- *(app)* Spawn ActionEngine Scheduler CommandParser on boot
- *(app)* Build Actions screen with tree pane and detail view
- *(app)* Add Add Action modal and create-action flow
- *(app)* Add Add Trigger modal with kind picker and config form
- *(app)* Add Add Sub-Action modal and remove button
- *(twitch)* Bridge chat.send.request events to Helix send

### 🚜 Refactor
- *(twitch)* [**breaking**] Hide SqliteBackend behind CredentialsRepo trait
- Strip stage refs from docs and drop unused import stub

### 🛠️ Build
- *(github)* Fix cargo dist
- *(github)* Undo running release on pull-request

### 🧪 Testing
- *(app)* Add end-to-end action pipeline causation test

## [0.1.0-alpha.3] - 2026-05-17
### ⚠️ BREAKING CHANGES
- **twitch**: TwitchChat / ChatSession / send_chat no longer take reqwest::Client parameter

### ⚙️ Miscellaneous Tasks
- *(cargo)* Cleanup
- Release

### 🐛 Bug Fixes
- *(app)* Wire user_info into credentials and chat operations
- *(app)* Bypass keyring in test helpers via open_with_key

### 📚 Documentation
- *(release)* Release v0.1.0-alpha.3

### 🚀 Features
- *(runtime)* Add EventBus with broadcast and bounded replay buffer
- *(twitch)* Add chat ingestion via EventSub and send-chat via Helix
- *(widgets)* Add chat row input bar and filter chip widgets
- *(app)* Wire LiveChat screen with EventBus subscription
- *(app)* Add Settings Platforms screen with Reconnect button
- *(twitch)* Add fetch_user_info via Helix GET users endpoint

### 🚜 Refactor
- *(twitch)* [**breaking**] Hide reqwest behind chat module internals
- *(events)* Remove dead EventBus trait and async-trait dep

### 🛠️ Build
- Drop redundant Swatinem cache step (setup-rust-toolchain handles it)
- Fix PR concurrency group and enable fail-fast on test matrix
- *(manual)* Tighten upload-artifact paths to exact binary names
- *(release)* Split build into per-OS matrix and download-merge publish

### 🧪 Testing
- *(twitch)* Add token redaction CI gate per RFC-011 qa concern
- *(app)* Add livechat end-to-end integration test

## [0.1.0-alpha.2] - 2026-05-17
### ⚠️ BREAKING CHANGES
- **widgets**: LoomPalette adds surface_overlay field
- **widgets**: Spacing scale + Density::Spacious rename + new Radius/BorderWidth/FontSize tokens
- **widgets**: onboarding widget signatures rebuilt — see DESIGN_AUDIT.md
- **oauth**: DeviceCodePoller methods no longer take reqwest::Client parameter

### ⚙️ Miscellaneous Tasks
- Rename project loom to forge across workspace
- Add MIT and Apache-2.0 dual license files
- *(github)* Add issue templates and pull request template
- Remove docs/ — CHANGELOG.md is the canonical release log
- Release

### 🐛 Bug Fixes
- *(widgets)* Migrate to iced 0.14 API after dep bump
- *(storage)* Register keyring backend and fall back to file key
- *(storage)* Rename LOOM_CREDENTIAL_KEY_FILE env var to FORGE prefix
- *(storage)* Bypass keyring in tests via open_with_key

### 📚 Documentation
- *(readme)* Capitalize Forge in acknowledgements sentence
- *(readme)* Add known limitations section for twitch client id
- Add CODE_OF_CONDUCT CONTRIBUTING and SECURITY policy
- *(readme)* Fix path to LICENSE
- *(release)* Release v0.1.0-alpha.2
- *(release)* Release v0.1.0-alpha.2

### 🚀 Features
- *(oauth)* Add DeviceCodePoller state machine with slow-down
- *(twitch)* Bootstrap loom-platform-twitch with device-code auth
- *(widgets)* Add onboarding widget family for first-run flow
- *(app)* Add OnboardingState and Welcome screen scaffold
- *(app)* Build ConnectPlatform screen with 4-card picker
- *(app)* Add ConnectObs StarterPack Ready onboarding screens
- *(app)* Persist onboarding_completed flag on finish or skip
- *(twitch)* Add client_id resolver with env and option_env
- *(app)* Wire DeviceCodeFlow screen with twitch oauth
- *(app)* Persist and resume last_onboarding_step across restarts

### 🚜 Refactor
- *(widgets)* [**breaking**] Fix theme background and add surface_overlay token
- *(widgets)* [**breaking**] Rebuild token scale with html-observed values
- *(widgets)* Rebuild button styles to match design source
- *(widgets)* Rebuild card styles to match design source
- *(widgets)* Align sidebar toolbar title-bar with design
- *(widgets)* Style text-input search-input and select
- *(widgets)* [**breaking**] Rewrite onboarding family to match mockups
- *(widgets)* Style section headers and add expandable variant
- *(widgets)* Rename LoomPalette to ForgePalette
- *(oauth)* [**breaking**] Hide reqwest behind DeviceCodePoller internals

### 🛠️ Build
- Split lint/test jobs, add audit, least-privilege perms
- Restore action version tags from dependabot bumps
- Inject FORGE_TWITCH_CLIENT_ID secret into release builds
- Add timeout-minutes guard on test jobs

## [0.1.0-alpha.1] - 2026-05-16
### ⚙️ Miscellaneous Tasks
- *(workspace)* Add cross-platform .gitignore
- *(workspace)* Bootstrap alpha-1 10-crate skeleton
- *(deps)* Bump softprops/action-gh-release from 2 to 3 (#2)
- *(deps)* Bump actions/upload-artifact from 4 to 7 (#3)
- *(deps)* Bump actions/checkout from 4 to 6 (#4)
- *(deps)* Bump iced from 0.13.1 to 0.14.0 (#5)
- *(deps)* Bump axum from 0.7.9 to 0.8.9 (#7)
- *(deps)* Bump tokio-tungstenite from 0.24.0 to 0.29.0 (#9)
- *(deps)* Bump keyring from 3.6.3 to 4.0.1 (#6)
- *(deps)* Bump iced_fonts from 0.1.1 to 0.3.0 (#8)
- Release

### 🎨 Styling
- *(types)* Strip tautological doc comments
- *(events)* Strip doc-policy violations
- *(widgets)* Collapse multi-line doc comments on font helpers
- *(workspace)* Fix formating

### 🐛 Bug Fixes
- *(storage)* Land queue.rs to match lib.rs mod declaration

### 📚 Documentation
- *(readme)* Add project README and alpha-1 release notes
- *(release)* Release v0.1.0-alpha.1

### 🚀 Features
- *(types)* Seed loom-types with Variant value system
- *(events)* Seed loom-events with Event bus contract
- *(storage)* Add StorageError enum with typed variants
- *(globals)* Add GlobalsRepo trait with GlobalEntry type
- *(storage)* Add UserGlobalsRepo trait for per-broadcaster scope
- *(storage)* Add SettingsRepo trait with reserved-keys catalog
- *(storage)* Add ActionRepo trait with ActionRecord type
- *(storage)* Add TriggerRepo trait with TriggerRecord type
- *(storage)* Add CommandRepo trait with CommandRecord type
- *(storage)* Add ScriptRepo trait with ScriptRecord type
- *(storage)* Add CredentialsRepo trait with CredentialId type
- *(storage)* Add HistoryRepo trait with HistoryRecord type
- *(storage)* Add DataProvider super-trait composing 10 repos
- *(platforms)* Add PlatformError enum with typed variants
- *(rhai)* Add ScriptError enum with sandbox-aware variants
- *(server)* Add ServerError enum with auth and sandbox variants
- *(oauth)* Add AuthFlow enum for device-code and local-callback
- *(platforms)* Add PlatformCapabilities with Limited flag
- *(server)* Add bindable axum stub with ServerHandle
- *(rhai)* Add sandboxed Engine wrapper with op-limit config
- *(platforms)* Add ChatPlatform trait with ConnectionState
- *(platforms)* Add RateLimiter trait with outcome enum
- *(platforms)* Add IntegrationDetail page trait family
- *(storage)* Add sqlite migration 0001 with all alpha-1 tables
- *(widgets)* Add LoomPalette and design tokens for 3 themes
- *(globals)* Implement SQLite GlobalsRepo with telemetry
- *(widgets)* Add Tier 1 button family with iced 0.13 styling
- *(widgets)* Add Tier 1 status indicator family
- *(widgets)* Add Tier 1 card family with metric + hero cards
- *(app)* Add iced application shell with Screen enum router
- *(storage)* Implement SQLite action-engine quad repo impls
- *(storage)* Implement AES-GCM crypto + CredentialsRepo
- *(storage)* Implement SQLite ScriptRepo and migration 0003
- *(storage)* Implement SQLite HistoryRepo
- *(storage)* Add SqliteBackend with DataProvider impl
- *(widgets)* Add Tier 1 navigation family with sidebar
- *(widgets)* Add Tier 1 layout family with title bar
- *(widgets)* Add Tier 1 input family with search and select
- *(widgets)* Add Tier 1 sections and notifications family
- *(hub)* Wire sidebar navigation and Hub view layout
- *(settings)* Add Settings sub-screens and Onboarding routing
- *(app)* Wire SqliteBackend into boot with first-run routing
- *(runtime)* Add InMemoryEventBus and iced subscription bridge

### 🛠️ Build
- *(github)* Add pr.yml workflow with cross-platform matrix
- *(github)* Add nightly workflow and dependabot config
- *(release)* Add release.yml manual.yml and cargo-dist config

