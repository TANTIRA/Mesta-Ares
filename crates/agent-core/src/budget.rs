//! Resource budgets bounding an agent run.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::state::AgentState;

/// Hard limits bounding a single agent run.
///
/// `max_cost_usd` is carried for callers that price tokens; the loop itself
/// enforces steps, tokens and wall time only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Budgets {
    /// Maximum number of [`crate::Step`]s the state may accumulate.
    pub max_steps: u32,
    /// Maximum total tokens (input + output) the run may consume.
    pub max_tokens: u64,
    /// Maximum wall-clock duration of the run, in seconds.
    pub wall_time_secs: u64,
    /// Maximum allowed spend; enforced by callers that price tokens.
    pub max_cost_usd: f64,
}

impl Default for Budgets {
    /// Defaults: 50 steps, 200k tokens, 600 seconds, $2.
    fn default() -> Self {
        Self {
            max_steps: 50,
            max_tokens: 200_000,
            wall_time_secs: 600,
            max_cost_usd: 2.0,
        }
    }
}

impl Budgets {
    /// Return the stop reason if `state` or elapsed time exceeds a limit.
    pub(crate) fn exceeded_reason(
        &self,
        state: &AgentState,
        started: Instant,
    ) -> Option<&'static str> {
        if state.steps.len() as u32 >= self.max_steps {
            Some("max_steps")
        } else if state.tokens_used >= self.max_tokens {
            Some("max_tokens")
        } else if started.elapsed().as_secs() >= self.wall_time_secs {
            Some("wall_time")
        } else {
            None
        }
    }
}
