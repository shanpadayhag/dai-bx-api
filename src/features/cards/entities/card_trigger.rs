use uuid::Uuid;

use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Started,
    Finished,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent<'a> {
    Started,
    Finished { uncheck_cutoffs: &'a UncheckCutoffs },
}

impl TriggerEvent<'_> {
    pub fn kind(self) -> TriggerKind {
        match self {
            Self::Started => TriggerKind::Started,
            Self::Finished { .. } => TriggerKind::Finished,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerTarget {
    pub list_id: Option<Uuid>,
    pub list_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardTriggers {
    pub started: Option<TriggerTarget>,
    pub finished: Option<TriggerTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerChoice {
    pub started_list_id: Option<Uuid>,
    pub finished_list_id: Option<Uuid>,
}
