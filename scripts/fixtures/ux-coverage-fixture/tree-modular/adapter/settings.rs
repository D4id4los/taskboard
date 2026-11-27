//! Fixture adapter for the ux-coverage.sh self-test.
fn on_fixture_tap() {
    // wiring W-1: the adapter-side handler the gate greps for
}
# (#1282) adapter-side snake spelling for the kebab slint callback
fn on_habit_edit_asked() {
    // the plan's snake cell resolves the adapter through the on_<cb> idiom
}
fn on_fixture_badge() {
    // wiring W-4 (INVENTORY-MISS variant): installed in the adapter but
    // deliberately absent from tree/seam_inventory.rs
}
fn on_save_extra() {
    // #965 prefixmiss variant: only this suffixed spelling exists, so a
    // plan row naming the shorter callback must NOT resolve.
}
fn on_load() {
    // #965 prefixfound variant: the honest handler, adjacent to the
    // suffixed spelling below.
}
fn on_load_extra() {
    // #965 prefixfound variant: the suffixed spelling must not satisfy
    // a plan row naming the shorter callback.
}
