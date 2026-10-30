msg-obs-connect_refused = OBS is not accepting connections. Start OBS and turn on its WebSocket server.
msg-obs-unreachable = OBS could not be reached. Check the address and the network.
msg-obs-password_rejected = OBS rejected the password. Enter the WebSocket password again.
msg-obs-connect_failed = Could not connect to OBS.
msg-obs-connection_lost = The connection to OBS was lost.
msg-obs-unresponsive = OBS stopped answering requests.
msg-obs-health_stream = Stream
msg-obs-health_recording = Recording
msg-obs-health_cpu_fps = CPU · FPS
msg-obs-health_dropped_frames = Dropped
msg-obs-not_connected = Not connected to OBS.
msg-obs-request_timed_out = OBS did not answer in time.
msg-obs-request_rejected = OBS rejected the { $request } request.
msg-obs-protocol_error = OBS sent a reply forge could not understand.
msg-obs-payload_invalid = The request data could not be encoded for OBS.
msg-obs-studio_mode_inactive = Studio Mode is off in OBS. Turn it on first with the "Set Studio Mode" sub-action.
msg-obs-health_live = Live
msg-obs-health_off = Off
msg-obs-health_paused = Paused
msg-obs-health_active = Active
msg-obs-health_lagging = lagging
msg-obs-health_smooth = smooth
msg-obs-health_cpu_fps_value = { NUMBER($cpu, minimumFractionDigits: 1, maximumFractionDigits: 1) }% · { NUMBER($fps, minimumFractionDigits: 1, maximumFractionDigits: 1) }
msg-obs-health_dropped_count = { $count ->
    [one] { $count } frame
   *[other] { $count } frames
}
msg-obs-health_dropped_share = { NUMBER($percent, minimumFractionDigits: 2, maximumFractionDigits: 2) }%
