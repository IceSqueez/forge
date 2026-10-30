#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingPortability {
    Portable,
    MachineLocal,
    RepositoryOwned,
}

impl SettingPortability {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::MachineLocal => "machine_local",
            Self::RepositoryOwned => "repository_owned",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Portable, Self::MachineLocal, Self::RepositoryOwned]
            .into_iter()
            .find(|class| class.code() == code)
    }
}
