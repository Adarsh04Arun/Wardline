//! LLM-oriented reference guards and the classifier adapter.
//!
//! The heuristic in [`PromptInjectionGuard`] is a cheap first line of
//! defense. It is not a substitute for a model-based classifier plugged in
//! through [`ClassifierAdapter`].

mod output_classifier;
mod prompt_injection;

pub use output_classifier::{Classification, ClassifierAdapter, OutputClassifier};
pub use prompt_injection::PromptInjectionGuard;
