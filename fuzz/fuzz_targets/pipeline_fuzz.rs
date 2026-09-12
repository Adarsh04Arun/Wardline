//! Fuzz pipeline construction and evaluation against arbitrary guard
//! behaviour (allow / block / error / panic, fail-open / fail-closed).
//!
//! Run periodically, not on every PR:
//!
//! ```sh
//! cargo +nightly fuzz run pipeline_fuzz
//! ```
//!
//! from the repo root after `cargo install cargo-fuzz`. This crate is its
//! own workspace so a normal `cargo test --workspace` never pulls in
//! libFuzzer.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::Arc;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Pipeline, Verdict};

const NAMES: [&str; 8] = ["g0", "g1", "g2", "g3", "g4", "g5", "g6", "g7"];

struct ByteGuard {
    name_idx: usize,
    code: u8,
}

impl Guard for ByteGuard {
    type Input = [u8];
    type Output = ();

    fn check(&self, _input: &[u8], _ctx: &Context) -> Result<Verdict, GuardError> {
        match self.code % 5 {
            0 => Ok(Verdict::Allow),
            1 => Ok(Verdict::block("fuzz")),
            2 => Err(GuardError::internal("fuzz")),
            3 => panic!("fuzz panic"),
            _ => Ok(Verdict::Modify(())),
        }
    }

    fn fail_policy(&self) -> FailPolicy {
        if self.code & 0x10 == 0 {
            FailPolicy::FailOpen
        } else {
            FailPolicy::FailClosed
        }
    }

    fn name(&self) -> &'static str {
        NAMES[self.name_idx]
    }
}

fuzz_target!(|data: &[u8]| {
    let mut pipeline: Pipeline<[u8], ()> = Pipeline::new().with_trace_capacity(4);
    for (index, &code) in data.iter().take(NAMES.len()).enumerate() {
        pipeline.push(ByteGuard {
            name_idx: index,
            code,
        });
    }
    let input = Arc::<[u8]>::from(data.to_vec());
    let _ = pipeline.evaluate(&input, &Context::new());
});
