#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationSettings {
    pub check_ins: bool,
    pub schedule_runs: bool,
    pub blocks_starting: bool,
    pub quiet_outside_working_hours: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NotificationChanges {
    pub check_ins: Option<bool>,
    pub schedule_runs: Option<bool>,
    pub blocks_starting: Option<bool>,
    pub quiet_outside_working_hours: Option<bool>,
}
