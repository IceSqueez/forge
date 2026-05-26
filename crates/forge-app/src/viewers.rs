use std::sync::Arc;

use forge_storage::{Viewer, ViewerRepo};
use iced::Task;

use crate::Message;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, Default)]
pub struct ViewersState {
    pub viewers: Vec<Viewer>,
}

#[derive(Debug, Clone)]
pub enum ViewersMsg {
    LoadRequested,
    Loaded(Result<Vec<Viewer>, String>),
}

pub async fn load_viewers(repo: Arc<dyn ViewerRepo>) -> Result<Vec<Viewer>, String> {
    repo.list().await.map_err(|e| e.to_string())
}

pub fn update(state: &mut ViewersState, rt: &RuntimeView, msg: ViewersMsg) -> Task<Message> {
    match msg {
        ViewersMsg::LoadRequested => {
            let repo = rt.backend.viewer_repo();
            Task::perform(load_viewers(repo), |r| {
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
