use std::sync::Arc;

use forge_events::Event;
use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use iced::Task;

use crate::message::{HomeMsg, HomeStatsData, Message};
use crate::runtime_view::RuntimeView;

#[derive(Default)]
pub struct HomeStats {
    pub actions_count: Option<usize>,
    pub commands_count: Option<usize>,
    pub triggers_fired: Option<u64>,
    pub globals_count: Option<usize>,
}

impl HomeStats {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn on_event(state: &mut HomeStats, event: &Event) -> Task<Message> {
    if event.kind == "action.done" {
        state.triggers_fired = Some(state.triggers_fired.unwrap_or(0) + 1);
    }
    Task::none()
}

pub fn update(state: &mut HomeStats, rt: &RuntimeView, msg: HomeMsg) -> Task<Message> {
    match msg {
        HomeMsg::LoadStats => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move { load_home_stats(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Home(HomeMsg::StatsLoaded(r)),
            )
        }
        HomeMsg::StatsLoaded(Ok(data)) => {
            state.actions_count = Some(data.actions_count);
            state.commands_count = Some(data.commands_count);
            state.triggers_fired = Some(data.triggers_fired);
            state.globals_count = Some(data.globals_count);
            Task::none()
        }
        HomeMsg::StatsLoaded(Err(e)) => {
            tracing::warn!(error = %e, "home stats load failed");
            Task::none()
        }
    }
}

async fn load_home_stats(dp: Arc<SqliteBackend>) -> Result<HomeStatsData, String> {
    let actions = dp
        .action_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let commands = dp
        .command_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let globals = dp.list().await.map_err(|e| e.to_string())?.len();
    let since = time::OffsetDateTime::now_utc() - time::Duration::hours(24);
    let stats = dp
        .history_repo()
        .stats_summary(since)
        .await
        .map_err(|e| e.to_string())?;
    let triggers_fired: u64 = stats.values().map(|s| u64::from(s.runs_24h)).sum();
    Ok(HomeStatsData {
        actions_count: actions,
        commands_count: commands,
        triggers_fired,
        globals_count: globals,
    })
}
