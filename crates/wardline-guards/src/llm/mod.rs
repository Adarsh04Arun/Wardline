//! LLM-oriented reference guards.
//!
//! The heuristic in [`PromptInjectionGuard`] is a cheap first line of
//! defense. It is not a substitute for a model-based classifier.

mod prompt_injection;

pub use prompt_injection::PromptInjectionGuard;
