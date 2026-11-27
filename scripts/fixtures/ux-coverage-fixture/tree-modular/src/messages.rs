// Production twin of the #1073 tier-sort fixture: the TierProbe
// token exists here AND in src/test_support/messages.rs — the command
// citation must deterministically pick this production file.
pub enum StateCommand {
    TierProbe,
}
