use std::sync::Arc;

use forge_storage::Viewer;
use forge_storage_sqlite::SqliteBackend;
use iced::Task;

use crate::Message;

#[derive(Debug, Clone, Default)]
pub struct ViewersState {
    pub viewers: Vec<Viewer>,
}

#[derive(Debug, Clone)]
pub enum ViewersMsg {
    LoadRequested,
    Loaded(Result<Vec<Viewer>, String>),
}

pub async fn load_viewers(dp: Arc<SqliteBackend>) -> Result<Vec<Viewer>, String> {
    use forge_storage::DataProvider;
    dp.viewer_repo().list().await.map_err(|e| e.to_string())
}

pub fn handle_msg(
    state: &mut ViewersState,
    msg: ViewersMsg,
    backend: &Arc<SqliteBackend>,
) -> Task<Message> {
    match msg {
        ViewersMsg::LoadRequested => {
            let dp = Arc::clone(backend);
            Task::perform(load_viewers(dp), |r| {
                Message::Viewers(ViewersMsg::Loaded(r))
            })
        }
        ViewersMsg::Loaded(Ok(v)) => {
            state.viewers = v;
            Task::none()
        }
        ViewersMsg::Loaded(Err(_)) => Task::none(),
    }
}
