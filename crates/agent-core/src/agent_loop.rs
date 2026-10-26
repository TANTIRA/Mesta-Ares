//! The checkpointed think-act-observe control loop.

use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;
use uuid::Uuid;

use agent_protocol::tool::ToolCall;
use llm_client::{CompletionRequest, LlmProvider, Message, Role, strong_model};

use agent_protocol::kill::kill_switch_engaged;

use crate::{
    budget::Budgets,
    checkpoint::CheckpointStore,
    error::AgentError,
    gateway::ToolGateway,
    state::{AgentState, Step},
};

/// JSON directive the model emits each iteration.
///
/// For `tool`, the loop fabricates the [`ToolCall`] envelope itself — the
/// model must never mint ids, actors or correlation ids.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Directive {
    Plan {
        text: String,
    },
    Tool {
        tool: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    Final {
        text: String,
    },
}

/// Result of an [`AgentLoop::run`] call.
pub struct LoopOutcome {
    /// Final agent state (also persisted to the checkpointer).
    pub state: AgentState,
    /// `true` when the run ended on a `final` directive.
    pub completed: bool,
    /// Why the loop stopped: `final`, `max_steps`, `max_tokens`,
    /// `wall_time`, or `kill_switch`.
    pub stop_reason: String,
}

impl LoopOutcome {
    /// `true` when the run stopped because a budget limit was hit.
    pub fn budget_exceeded(&self) -> bool {
        !self.completed
            && matches!(
                self.stop_reason.as_str(),
                "max_steps" | "max_tokens" | "wall_time"
            )
    }
}

/// The agent control loop: LLM + tool gateway + checkpoint store.
pub struct AgentLoop {
    /// Provider driving reasoning steps.
    pub provider: Arc<dyn LlmProvider>,
    /// Gateway through which tool calls execute.
    pub gateway: Arc<dyn ToolGateway>,
    /// Store used to checkpoint state each iteration.
    pub checkpointer: Arc<dyn CheckpointStore>,
}

impl AgentLoop {
    /// Create a loop from its three collaborators.
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        gateway: Arc<dyn ToolGateway>,
        checkpointer: Arc<dyn CheckpointStore>,
    ) -> Self {
        Self {
            provider,
            gateway,
            checkpointer,
        }
    }

    /// Run the loop for `thread_id` until a stop condition.
    ///
    /// Each iteration: enforce budgets, checkpoint, send the serialized
    /// transcript to the provider, parse a JSON directive, act on it. Tool
    /// directives additionally check the kill switch before dispatch. The
    /// final state is always persisted before returning.
    pub async fn run(
        &self,
        thread_id: Uuid,
        input: impl Into<String>,
        system_prompt: impl Into<String>,
        budgets: Budgets,
    ) -> Result<LoopOutcome, AgentError> {
        let started = Instant::now();
        let system_prompt = system_prompt.into();

        let mut state = self
            .checkpointer
            .load(thread_id)
            .await?
            .unwrap_or_else(|| AgentState {
                thread_id,
                input: input.into(),
                steps: Vec::new(),
                tokens_used: 0,
            });

        let (state, completed, stop_reason) = loop {
            if let Some(reason) = budgets.exceeded_reason(&state, started) {
                break (state, false, reason.to_string());
            }

            self.checkpointer.save(&state).await?;

            let req = CompletionRequest {
                model: strong_model(),
                system: Some(system_prompt.clone()),
                messages: vec![Message {
                    role: Role::User,
                    content: render_transcript(&state)?,
                }],
                max_tokens: 2048,
                temperature: Some(0.0),
            };
            let resp = self.provider.complete(&req).await?;
            state.tokens_used = state
                .tokens_used
                .saturating_add(resp.input_tokens + resp.output_tokens);

            match parse_directive(&resp.text) {
                Ok(Directive::Plan { text }) => {
                    state.steps.push(Step::Plan { text });
                }
                Ok(Directive::Tool { tool, args }) => {
                    if kill_switch_engaged() {
                        tracing::warn!(tool = %tool, "tool call blocked: kill switch engaged");
                        state.steps.push(Step::Observation {
                            text: format!("tool call `{tool}` blocked: kill switch engaged"),
                        });
                        break (state, false, "kill_switch".to_string());
                    }
                    let call = ToolCall {
                        id: Uuid::new_v4(),
                        tool,
                        args,
                        actor: "agent-core".to_string(),
                        idempotency_key: Uuid::new_v4().to_string(),
                        correlation_id: thread_id,
                    };
                    let result = self.gateway.invoke(&call).await?;
                    let observation = serde_json::to_string(&result)?;
                    state.steps.push(Step::ToolCall { call });
                    state.steps.push(Step::Observation { text: observation });
                }
                Ok(Directive::Final { text }) => {
                    state.steps.push(Step::Final { text });
                    break (state, true, "final".to_string());
                }
                Err(err) => {
                    state.steps.push(Step::Observation {
                        text: format!(
                            "directive parse error: {err}; reply with one JSON directive"
                        ),
                    });
                }
            }
        };

        self.checkpointer.save(&state).await?;
        Ok(LoopOutcome {
            state,
            completed,
            stop_reason,
        })
    }
}

fn render_transcript(state: &AgentState) -> Result<String, AgentError> {
    let steps = serde_json::to_string_pretty(&state.steps)?;
    Ok(format!(
        "Input: {input}\n\nSteps so far:\n{steps}\n\n\
         Respond with exactly one JSON directive: \
         {{\"action\":\"plan\",\"text\":...}}, \
         {{\"action\":\"tool\",\"tool\":NAME,\"args\":{{...}}}}, or \
         {{\"action\":\"final\",\"text\":...}}.",
        input = state.input
    ))
}

/// Extract the outermost `{...}` span and parse it as a [`Directive`].
fn parse_directive(text: &str) -> Result<Directive, serde_json::Error> {
    let t = text.trim();
    let json = match (t.find('{'), t.rfind('}')) {
        (Some(s), Some(e)) if e > s => &t[s..=e],
        _ => t,
    };
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_protocol::tool::ToolResult;
    use llm_client::MockProvider;

    use crate::checkpoint::InMemoryCheckpointer;

    struct EchoGateway;

    #[async_trait::async_trait]
    impl ToolGateway for EchoGateway {
        async fn invoke(&self, call: &ToolCall) -> Result<ToolResult, AgentError> {
            Ok(ToolResult {
                call_id: call.id,
                ok: true,
                output: serde_json::json!({ "echo": call.args }),
                error: None,
                dry_run: false,
            })
        }
    }

    fn test_loop(reply: &str) -> AgentLoop {
        AgentLoop::new(
            Arc::new(MockProvider::new(reply)),
            Arc::new(EchoGateway),
            Arc::new(InMemoryCheckpointer::new()),
        )
    }

    #[tokio::test]
    async fn run_completes_on_final_directive() {
        let agent = test_loop(r#"{"action":"final","text":"done"}"#);
        let outcome = agent
            .run(Uuid::new_v4(), "hi", "sys", Budgets::default())
            .await
            .unwrap();
        assert!(outcome.completed);
        assert_eq!(outcome.stop_reason, "final");
        assert!(!outcome.budget_exceeded());
        assert!(matches!(
            outcome.state.steps.last(),
            Some(Step::Final { text }) if text == "done"
        ));
    }

    #[tokio::test]
    async fn run_stops_at_max_steps() {
        let agent = test_loop(r#"{"action":"tool","tool":"search","args":{}}"#);
        let budgets = Budgets {
            max_steps: 4,
            ..Budgets::default()
        };
        let outcome = agent
            .run(Uuid::new_v4(), "hi", "sys", budgets)
            .await
            .unwrap();
        assert!(!outcome.completed);
        assert_eq!(outcome.stop_reason, "max_steps");
        assert!(outcome.budget_exceeded());
    }

    #[test]
    fn parse_directive_ignores_code_fences() {
        let d = parse_directive("```json\n{\"action\":\"plan\",\"text\":\"x\"}\n```").unwrap();
        assert!(matches!(d, Directive::Plan { text } if text == "x"));
    }
}
