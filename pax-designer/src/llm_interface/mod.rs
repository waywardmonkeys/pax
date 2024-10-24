use std::sync::atomic::{AtomicBool, Ordering};

use crate::model;
use pax_engine::api::*;
use pax_engine::*;

use pax_std::*;

use crate::model::{
    action::{orm::SerializeRequested, Action, ActionContext},
    input::InputEvent,
};
#[pax]
#[engine_import_path("pax_engine")]
#[file("llm_interface/mod.pax")]
pub struct LLMInterface {
    pub visible: Property<bool>,
    pub request: Property<String>,
}

pub struct SetLLMPromptState(pub bool);

impl Action for SetLLMPromptState {
    fn perform(&self, ctx: &mut ActionContext) -> anyhow::Result<()> {
        SerializeRequested.perform(ctx)?;
        OPEN_LLM_PROMPT_PROP.with(|p| p.set(self.0));
        Ok(())
    }
}

thread_local! {
    static OPEN_LLM_PROMPT_PROP: Property<bool> = Property::new(false);
}

impl LLMInterface {
    pub fn on_mount(&mut self, _ctx: &NodeContext) {
        let state = OPEN_LLM_PROMPT_PROP.with(|p| p.clone());
        let deps = [state.untyped()];
        self.visible
            .replace_with(Property::computed(move || state.get(), &deps));
    }

    pub fn textbox_change(&mut self, ctx: &NodeContext, args: Event<TextboxChange>) {
        model::perform_action(&SetLLMPromptState(false), ctx);
        self.request.set(String::new());
        let request = &args.text;
        let mut dt = borrow_mut!(ctx.designtime);
        if let Err(e) = dt.llm_request(request) {
            pax_engine::log::warn!("llm request failed: {:?}", e);
        };
    }

    pub fn hide(&mut self, ctx: &NodeContext, event: Event<Click>) {
        model::perform_action(&SetLLMPromptState(false), ctx);
    }
}
