fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("streamer-loom starting");
}
