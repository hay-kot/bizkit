//! The analytics events other parts of bizkit can react to.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::Event;
use serde::{Deserialize, Serialize};

use crate::api::GoalId;

/// Published in the ingest transaction when a person converts on a goal for
/// the first time in a UTC day.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalConverted {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The project.
    pub project_id: ProjectId,
    /// The goal.
    pub goal_id: GoalId,
    /// The `analytics_events` row that converted.
    pub event_id: i64,
}

impl Event for GoalConverted {
    const NAME: &'static str = "analytics.goal.converted";
}
