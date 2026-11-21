// Test-support twin of the #1073 tier-sort fixture: same TierProbe
// token as the production twin; this file must never be cited while a
// production twin matches.
pub enum StateCommand {
    TierProbe,
}
