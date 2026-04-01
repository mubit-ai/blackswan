use crate::types::Staleness;
use std::time::SystemTime;

/// Compute staleness level from a memory's modified time.
#[allow(dead_code)]
pub fn compute_staleness(modified: SystemTime) -> Staleness {
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    let age_days = age.as_secs() / 86400;

    if age_days < 2 {
        Staleness::Fresh
    } else {
        Staleness::Warning { age_days }
    }
}

/// Generate a staleness warning string for display.
#[allow(dead_code)]
pub fn staleness_warning(staleness: Staleness) -> Option<String> {
    match staleness {
        Staleness::Fresh => None,
        Staleness::Warning { age_days } => Some(format!(
            "This memory is {age_days} days old. Memories are point-in-time observations, \
             not live state — claims about code or file references may be outdated. \
             Verify against current state before asserting as fact."
        )),
    }
}

/// Return the verification instructions to inject into the system prompt.
#[allow(dead_code)]
pub fn verification_instructions() -> &'static str {
    "A memory that names a specific file, function, endpoint, or configuration value \
     is a claim that it existed *when the memory was written*. It may have been renamed, \
     removed, or changed. Before recommending based on memory:\n\n\
     - If the memory names a file path: check the file exists.\n\
     - If the memory names a function or API: search for it.\n\
     - If the user is about to act on your recommendation: verify first.\n\n\
     \"The memory says X exists\" is not the same as \"X exists now.\"\n\n\
     A memory that summarizes system state is frozen in time. If the user asks about \
     *current* state, verify by reading the actual source of truth."
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fresh_within_one_day() {
        let now = SystemTime::now();
        assert_eq!(compute_staleness(now), Staleness::Fresh);

        let one_day_ago = now - Duration::from_secs(86400);
        assert_eq!(compute_staleness(one_day_ago), Staleness::Fresh);
    }

    #[test]
    fn warning_after_two_days() {
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 86400);
        assert_eq!(
            compute_staleness(two_days_ago),
            Staleness::Warning { age_days: 2 }
        );
    }

    #[test]
    fn warning_text_exists_for_stale() {
        assert!(staleness_warning(Staleness::Fresh).is_none());
        assert!(staleness_warning(Staleness::Warning { age_days: 5 }).is_some());
    }
}
