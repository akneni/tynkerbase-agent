use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Default)]
pub struct GlobalState {
    pub node_id: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub pass_sha256: Option<String>,
    pub pass_sha384: Option<String>,
    pub tyb_apikey: Option<String>,
}

pub type TsGlobalState = Arc<RwLock<GlobalState>>;

impl GlobalState {
    pub fn new() -> TsGlobalState {
        Arc::new(RwLock::new(Self::default()))
    }

    pub fn check_status(&self) -> bool {
        self.node_id.is_some()
            && self.name.is_some()
            && self.email.is_some()
            && self.pass_sha256.is_some()
            && self.pass_sha384.is_some()
            && self.tyb_apikey.is_some()
    }
}
