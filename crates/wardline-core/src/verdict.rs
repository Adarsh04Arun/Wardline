//! The answer a guard returns when it is asked about an action.

/// The outcome of a single guard evaluation.
///
/// `O` is the payload type for guards that rewrite rather than block; it
/// defaults to `()` for guards that only allow or block.
///
/// # Examples
///
/// ```
/// use wardline_core::Verdict;
///
/// let no: Verdict = Verdict::block("contains a banned phrase");
/// assert_eq!(no.block_reason(), Some("contains a banned phrase"));
///
/// let edited: Verdict<String> = Verdict::Modify("[redacted]".to_owned());
/// assert_eq!(edited.modified(), Some(&"[redacted]".to_owned()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Verdict<O = ()> {
    /// The action may proceed unchanged.
    Allow,
    /// The action must not happen. The pipeline stops here.
    Block {
        /// Why the action was refused. Ends up in the audit trace, so write
        /// it for whoever reads the log at 3am.
        reason: String,
    },
    /// The action may proceed, but with this replacement payload.
    ///
    /// Later guards still see the original input; composing modifications is
    /// the caller's decision, not the pipeline's.
    Modify(O),
}

impl<O> Verdict<O> {
    /// Builds a [`Verdict::Block`] from anything string-like.
    pub fn block(reason: impl Into<String>) -> Self {
        Verdict::Block {
            reason: reason.into(),
        }
    }

    /// Returns `true` if this verdict allows the action unchanged.
    pub fn is_allow(&self) -> bool {
        matches!(self, Verdict::Allow)
    }

    /// Returns `true` if this verdict refuses the action.
    pub fn is_block(&self) -> bool {
        matches!(self, Verdict::Block { .. })
    }

    /// Returns `true` if this verdict replaces the payload.
    pub fn is_modify(&self) -> bool {
        matches!(self, Verdict::Modify(_))
    }

    /// The block reason, or `None` for any other verdict.
    pub fn block_reason(&self) -> Option<&str> {
        match self {
            Verdict::Block { reason } => Some(reason),
            _ => None,
        }
    }

    /// The replacement payload, or `None` for any other verdict.
    pub fn modified(&self) -> Option<&O> {
        match self {
            Verdict::Modify(output) => Some(output),
            _ => None,
        }
    }

    /// Applies `f` to the replacement payload, leaving allow and block alone.
    pub fn map<T, F>(self, f: F) -> Verdict<T>
    where
        F: FnOnce(O) -> T,
    {
        match self {
            Verdict::Allow => Verdict::Allow,
            Verdict::Block { reason } => Verdict::Block { reason },
            Verdict::Modify(output) => Verdict::Modify(f(output)),
        }
    }
}

impl<O> Default for Verdict<O> {
    /// Allow — for guards that build a verdict incrementally, starting from
    /// "nothing objectionable found".
    ///
    /// This is not the library's failure default; that is [`FailPolicy`],
    /// which fails closed.
    ///
    /// [`FailPolicy`]: crate::FailPolicy
    fn default() -> Self {
        Verdict::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_builds_from_str_and_string() {
        let from_str: Verdict = Verdict::block("nope");
        let from_string: Verdict = Verdict::block(String::from("nope"));
        assert_eq!(from_str, from_string);
    }

    #[test]
    fn accessors_only_answer_for_their_own_variant() {
        let allow: Verdict<String> = Verdict::Allow;
        let block: Verdict<String> = Verdict::block("bad");
        let modify: Verdict<String> = Verdict::Modify("clean".to_owned());

        assert!(allow.is_allow() && !allow.is_block() && !allow.is_modify());
        assert!(block.is_block() && !block.is_allow() && !block.is_modify());
        assert!(modify.is_modify() && !modify.is_allow() && !modify.is_block());

        assert_eq!(block.block_reason(), Some("bad"));
        assert_eq!(allow.block_reason(), None);
        assert_eq!(modify.block_reason(), None);

        assert_eq!(modify.modified(), Some(&"clean".to_owned()));
        assert_eq!(allow.modified(), None);
        assert_eq!(block.modified(), None);
    }

    #[test]
    fn map_rewrites_only_the_modify_payload() {
        let modify: Verdict<&str> = Verdict::Modify("clean");
        assert_eq!(
            modify.map(str::to_owned),
            Verdict::Modify("clean".to_owned())
        );

        let block: Verdict<&str> = Verdict::block("bad");
        assert_eq!(block.map(str::to_owned), Verdict::block("bad"));

        let allow: Verdict<&str> = Verdict::Allow;
        assert_eq!(allow.map(str::to_owned), Verdict::Allow);
    }

    #[test]
    fn default_is_allow() {
        assert_eq!(Verdict::<()>::default(), Verdict::Allow);
    }
}
