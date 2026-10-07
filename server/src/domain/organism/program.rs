use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, fmt, path::PathBuf};
use uuid::Uuid;

/// 玩家程序的稳定 ID，用于和生物实例配对。
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OrganismProgramId(Uuid);

impl OrganismProgramId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for OrganismProgramId {
    fn default() -> Self {
        Self::new()
    }
}

impl Borrow<Uuid> for OrganismProgramId {
    fn borrow(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for OrganismProgramId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// 玩家程序目录记录。服务端只保存记录，不读取或执行目录中的代码。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProgramFolderRecord {
    pub program_id: OrganismProgramId,
    pub folder: PathBuf,
    pub entry_file: String,
}

impl ProgramFolderRecord {
    pub fn new(program_id: OrganismProgramId, folder: impl Into<PathBuf>) -> Self {
        Self {
            program_id,
            folder: folder.into(),
            entry_file: "main.py".to_string(),
        }
    }

    pub fn entry_path(&self) -> PathBuf {
        self.folder.join(&self.entry_file)
    }
}

/// 与生物实例关联的玩家程序。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrganismProgram {
    pub record: ProgramFolderRecord,
}
