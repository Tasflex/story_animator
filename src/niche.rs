//! The channel's niche: what we make videos about, and what we turn down.
//!
//! Targeting everything is targeting nothing. Every topic is scored against
//! this brief while the video is planned (topics below MIN_TOPIC_FIT are
//! refused unless forced), and `story_animator topics` proposes topics that
//! play to the engine's strengths: mechanisms shown step by step, flows,
//! networks, true-scale comparisons and big numbers.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::llm;
use crate::storyboard::json_body;

pub const BRIEF: &str = r#"CHANNEL: engineering marvels and how things work, explained with animated diagrams
(think Practical Engineering meets 3Blue1Brown). Viewers are curious adults and teens who love
"how on earth does that work?" moments.

Great topics, because our animation engine shows them best:
- machines and mechanisms: jet engines, elevators, locks, mechanical watches, wind turbines, brakes
- megastructures and infrastructure: bridges, dams, skyscrapers, tunnels, canals, power grids,
  undersea cables, ports, railways
- systems of flows and steps: the internet, GPS, water treatment, air traffic control, rockets
  reaching orbit, how a chip is made
- nature's engineering: how birds fly, how the heart pumps, termite mound cooling, spider silk
- physics inside everyday technology: microwaves, touchscreens, fridges, noise cancelling, batteries

Every video must have a mechanism to show step by step, real numbers and true-scale comparisons,
and at least three "wow" moments: records, fun facts, or clever engineering tricks that make
people say "wait, really?".

Poor fits, refuse them: news, politics, celebrities, opinions or debates, personal advice (health,
money, relationships), history told as a story with no mechanism, trivia lists with no through-line,
anything that needs real photos or footage to make sense."#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIdea {
    pub topic: String,
    #[serde(default)]
    pub hook: String,
    #[serde(default)]
    pub wow: Vec<String>,
    #[serde(default)]
    pub why: String,
}

#[derive(Deserialize)]
struct TopicList {
    topics: Vec<TopicIdea>,
}

/// Ask the LLM for topic ideas that fit the channel.
pub async fn suggest_topics(client: &Client, cfg: &Config, count: usize, area: Option<&str>) -> AppResult<Vec<TopicIdea>> {
    if !cfg.has_llm() {
        return Err(AppError::Config("no LLM key configured: set GEMINI_API_KEY or DEEPSEEK_API_KEY".into()));
    }
    let count = count.clamp(1, 30);
    let system = format!(
        "You are the head of development for a YouTube channel.\n\n{BRIEF}\n\n\
         Propose video topics for five-minute animated explainers. Each must fit the channel brief, have a \
         clear mechanism to animate, and carry at least three concrete wow facts you are confident are true. \
         Avoid generic titles: be specific (\"How the Golden Gate Bridge holds 887,000 tonnes\" beats \"Bridges\").\n\
         Return ONE JSON object, no prose: {{\"topics\": [{{\"topic\": \"specific topic\", \
         \"hook\": \"one-sentence cold open\", \"wow\": [\"fact\", \"fact\", \"fact\"], \
         \"why\": \"why it suits animated diagrams\"}}]}}"
    );
    let user = match area {
        Some(a) => format!("Propose {count} topics in this area: {a}"),
        None => format!("Propose {count} topics across the channel's range."),
    };
    let raw = llm::complete(client, cfg, &system, &user, true).await?;
    let list: TopicList = serde_json::from_str(json_body(&raw).map_err(AppError::Llm)?)
        .map_err(|e| AppError::Llm(format!("topic ideas: invalid JSON: {e}")))?;
    Ok(list.topics.into_iter().take(count).collect())
}
