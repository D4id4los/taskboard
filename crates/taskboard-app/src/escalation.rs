// SPDX-License-Identifier: AGPL-3.0-only
//! The reminder escalation driver (DR-3/4, D-I5's driver half /
//! D-I9): one app-layer process glue task beside the notifier that
//! keeps an `alarm`-insistent awaiting occurrence in the user's face —
//! re-toasts on a cadence through the existing [`crate::notifications::NotificationSink`]
//! port, and a global owned chime through the [`ChimeSink`] seam.
//!
//! Fences (dispatch §11.3, stage-1 plan §1): no engine change, no new
//! channel topology, no `SystemEvent`/`EngineSignal` variant — the
//! driver only reads the published-state `ArcSwap` and parks on the
//! existing `EngineSignal` broadcast. All audio lives behind
//! [`ChimeSink`] (the workspace's only rodio touchpoint); a failure is
//! logged and degrades to visual-only, never fatal. The cadence and
//! volume constants are named here, never config (D-I9's recorded
//! fence) — the ladder itself is per-reminder insistence data.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::broadcast;

use taskboard_domain::{
    AppState, Clock, EngineSignal, Insistence, Notification, ReminderDefaults, ReminderRef,
};

use crate::notifications::NotificationSink;

// -- the ladder's named constants (the one cite site) -----------------

/// Re-toast #1 opens at t₀ + 1 min (the review fence: "t0 fire, +1 min
/// sound start, 2-min re-toast" — both insistence channels open at +1
/// min; the fire toast itself is the engine's E-20 delivery).
pub(crate) const RE_TOAST_FIRST: TimeDelta = TimeDelta::minutes(1);
/// The re-toast cadence after the opener: t₀+3m, t₀+5m, …
pub(crate) const RE_TOAST_EVERY: TimeDelta = TimeDelta::minutes(2);
/// The global chime's opener (earliest awaiting t₀ + 1 min).
pub(crate) const CHIME_FIRST: TimeDelta = TimeDelta::minutes(1);
/// The chime cadence after the opener: every 30 s.
pub(crate) const CHIME_EVERY: TimeDelta = TimeDelta::seconds(30);
/// The volume ramp's full-volume bound (earliest t₀ + 5 min).
pub(crate) const CHIME_RAMP: TimeDelta = TimeDelta::minutes(5);
/// The ramp's floor volume.
pub(crate) const CHIME_LO: f32 = 0.30;
/// The ramp's ceiling (and the hard cap after the ramp).
pub(crate) const CHIME_HI: f32 = 1.0;

// -- the audio seam (D-13's confinement) ------------------------------

/// The audio failure class (D-8): typed, logged-and-dropped by the
/// driver, never fatal, never a panic. The rodio cause's `Display`
/// rides along at the single wrap site — the enum is the workspace's
/// only audio error surface. Per-chime playback is infallible by
/// construction under rodio 0.23's `Player::connect_new` + `append`
/// path (no fallible step between), so the only failure is the open.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AudioError {
    /// The process's default audio sink could not be opened.
    #[error("the default audio sink could not be opened: {0}")]
    Open(String),
}

/// The audio seam: one chime at a volume. The only rodio touchpoint in
/// the workspace; a backend swap never touches surfaces or engine.
pub(crate) trait ChimeSink: Send + Sync + std::fmt::Debug {
    /// A failed chime is logged-and-dropped by the caller (typed, never
    /// fatal).
    ///
    /// # Errors
    /// [`AudioError`] when the backend refused the chime; the driver
    /// logs it and continues its cadence.
    fn chime(&self, volume: f32) -> Result<(), AudioError>;
}

/// The degrade-to-visual-only impl (D-8): constructed when the real
/// audio path is unavailable (no Desktop sink, or a failed open). A
/// no-op that cannot fail — the caller's "typed Ok-ish no-op".
#[derive(Debug, Default)]
pub(crate) struct DisabledChime;

impl ChimeSink for DisabledChime {
    fn chime(&self, _volume: f32) -> Result<(), AudioError> {
        Ok(())
    }
}

/// The disabled no-op sink. Construction is the process's one degrade
/// decision; `chime` never fails afterwards.
pub(crate) fn disabled_chime() -> Arc<dyn ChimeSink> {
    Arc::new(DisabledChime)
}

/// The real chime: the two-tone sample buffer is synthesized once at
/// construction (volume-free — D-14); each chime clones it into a
/// [`rodio::buffer::SamplesBuffer`] and plays it through the process's
/// default sink. The latest [`rodio::Player`] handle is held, replacing
/// any still-held one (a chime that overlaps its predecessor replaces
/// it).
pub(crate) struct RodioChime {
    /// Keeps the output stream (and the mixer) alive for the process
    /// lifetime; [`rodio::stream::MixerDeviceSink::mixer`] reads off it.
    sink: rodio::stream::MixerDeviceSink,
    /// The synthesized buffer, built once.
    samples: Vec<f32>,
    /// The latest player (the volume + replacement surface).
    player: std::sync::Mutex<Option<rodio::Player>>,
}

impl std::fmt::Debug for RodioChime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RodioChime")
            .field("samples", &self.samples.len())
            .finish_non_exhaustive()
    }
}

impl RodioChime {
    /// Opens the process's default audio sink and synthesizes the chime
    /// buffer. Called once, only when `AppMode::Desktop ∧ SinkKind::Desktop`
    /// (D-9) — a typed failure degrades to [`ChimeSink::disabled`] at
    /// the caller (`mount_chime`), never retried for the process
    /// lifetime.
    ///
    /// # Errors
    /// [`AudioError::Open`] when the default sink could not be opened.
    pub(crate) fn open() -> Result<Self, AudioError> {
        Self::open_with(rodio::stream::DeviceSinkBuilder::open_default_sink)
    }

    /// The injection seam (the tests feed a failing open; the typed
    /// [`rodio::stream::DeviceSinkError::NoDevice`] variant is the
    /// constructible class).
    fn open_with<F>(open: F) -> Result<Self, AudioError>
    where
        F: FnOnce() -> Result<rodio::stream::MixerDeviceSink, rodio::stream::DeviceSinkError>,
    {
        let sink = open().map_err(|err| AudioError::Open(err.to_string()))?;
        Ok(Self {
            sink,
            samples: chime_samples(),
            player: std::sync::Mutex::new(None),
        })
    }
}

impl ChimeSink for RodioChime {
    fn chime(&self, volume: f32) -> Result<(), AudioError> {
        let buffer = rodio::buffer::SamplesBuffer::new(
            std::num::NonZeroU16::new(1).expect("mono channel count"),
            std::num::NonZeroU32::new(CHIME_SAMPLE_RATE).expect("non-zero sample rate"),
            self.samples.clone(),
        );
        // The raw-sample path: `stream::play` would route through the
        // decoder (its `Read + Seek` bound) — the synthesized buffer is
        // a `Source` already, connected straight onto the mixer. The
        // only fallible step is the connection, which cannot fail (an
        // attached mixer never errors at `connect_new`), so a per-chime
        // failure class stays `AudioError`-free by construction.
        let player = rodio::Player::connect_new(self.sink.mixer());
        player.append(buffer);
        player.set_volume(volume.clamp(0.0, 1.0));
        // Replace the still-held predecessor: overlapping chimes do not
        // stack, the latest wins (the re-toast's replacement spirit on
        // the audio channel).
        *self
            .player
            .lock()
            .expect("chime player mutex poisoned by a panic while held") = Some(player);
        Ok(())
    }
}

/// The boot-time audio decision (D-9): the real chime only when the
/// mode is Desktop **and** the resolved sink is Desktop; otherwise (or
/// on a typed open failure, logged once) the disabled no-op — the
/// degrade-to-visual-only path. `open` is the construction seam the
/// tests inject into.
pub(crate) fn mount_chime(
    audio_allowed: bool,
    open: impl FnOnce() -> Result<RodioChime, AudioError>,
) -> Arc<dyn ChimeSink> {
    if !audio_allowed {
        return disabled_chime();
    }
    match open() {
        Ok(chime) => Arc::new(chime),
        Err(err) => {
            tracing::warn!(%err, "audio unavailable; alarm escalation degrades to visual-only");
            disabled_chime()
        }
    }
}

// -- synthesis (D-14) -------------------------------------------------

/// The chime's sample rate (mono).
pub(crate) const CHIME_SAMPLE_RATE: u32 = 44_100;
/// The chime's total length: 0.6 s two-tone pattern at the rate.
pub(crate) const CHIME_TOTAL_SAMPLES: usize = 26_460;
/// The raised-cosine fade window (both ends): 10 ms — the speaker
/// never clicks.
pub(crate) const CHIME_FADE_SAMPLES: usize = 441;
/// The first tone (E5) — the first half of the pattern.
pub(crate) const CHIME_TONE_A_HZ: f32 = 659.25;
/// The second tone (A5) — the second half.
pub(crate) const CHIME_TONE_B_HZ: f32 = 880.0;
/// The waveform's peak amplitude (half-scale; structural peak ≤ 1.0).
const CHIME_AMPLITUDE: f32 = 0.5;

/// The pure waveform: mono 44.1 kHz, ~0.6 s two-tone sine (E5 → A5,
/// one tone per half), 10 ms raised-cosine fade-in/out. Volume-free —
/// the ramp is applied at playback via the player's volume.
#[must_use]
pub(crate) fn chime_samples() -> Vec<f32> {
    let total = CHIME_TOTAL_SAMPLES;
    let fade_len = CHIME_FADE_SAMPLES;
    let half = total / 2;
    let rate = CHIME_SAMPLE_RATE;
    (0..total)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)] // a sample index, far below f32 mantissa reach
            let t = i as f32 / rate as f32;
            let omega = if i < half {
                CHIME_TONE_A_HZ
            } else {
                CHIME_TONE_B_HZ
            };
            let tone = (std::f32::consts::TAU * omega * t).sin();
            // Raised-cosine ramps: cos² shaped, 1 in the interior.
            let fade_in = if i < fade_len {
                #[allow(clippy::cast_precision_loss)] // a fade fraction in [0, 1)
                let x = i as f32 / fade_len as f32;
                0.5 - 0.5 * (std::f32::consts::PI * x).cos()
            } else {
                1.0
            };
            let fade_out = if i >= total - fade_len {
                #[allow(clippy::cast_precision_loss)] // a fade fraction in (0, 1]
                let x = (total - i) as f32 / fade_len as f32;
                0.5 - 0.5 * (std::f32::consts::PI * x).cos()
            } else {
                1.0
            };
            CHIME_AMPLITUDE * tone * fade_in * fade_out
        })
        .collect()
}

// -- the awaiting predicate (derived, never stored) -------------------

/// Every `alarm`-insistent notification currently awaiting the user:
/// the domain's own derivation, filtered one step. Insistence never
/// influences *when* something fires — only what escalates after.
#[must_use]
pub(crate) fn awaiting_alarms(
    state: &AppState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Vec<Notification> {
    taskboard_domain::active_notifications(&state.tasks, &state.reminders, defaults, now)
        .into_iter()
        .filter(|note| note.insistence == Insistence::Alarm)
        .collect()
}

// -- the pure ladder (D-I9) -------------------------------------------

/// One escalation action the ladder is due to take.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EscalationAction {
    /// Withdraw-then-deliver this occurrence's toast again.
    ReToast(Notification),
    /// One chime, at this volume (0.0..=1.0, clamped at the seam).
    Chime { volume: f32 },
}

/// What is due NOW, and when to wake next: the pure ladder's output.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct StepOutcome {
    /// At most one `ReToast` per awaiting occurrence and one `Chime` in
    /// total, per call.
    pub(crate) actions: Vec<EscalationAction>,
    /// The next instant any ladder rule fires; `None` ⇒ the caller must
    /// not arm a timer (park on signals only).
    pub(crate) next_due: Option<DateTime<Utc>>,
}

/// The driver's bookkeeping: per-occurrence last re-toast instant, and
/// the global last-chime instant (sound is one channel — D-4).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct LadderMemory {
    last_re_toast: HashMap<(ReminderRef, DateTime<Utc>), DateTime<Utc>>,
    last_chime: Option<DateTime<Utc>>,
}

impl LadderMemory {
    /// Retains only awaiting keys (D-12): a dismissed/snoozed
    /// occurrence's cadence dies with its membership.
    fn evict(&mut self, awaiting: &[Notification]) {
        self.last_re_toast.retain(|key, _| {
            awaiting
                .iter()
                .any(|note| note.reminder == key.0 && note.occurrence == key.1)
        });
    }

    /// The earliest awaiting t₀ (the chime schedule's and ramp's key —
    /// D-4: one global sound stream off the earliest occurrence).
    fn earliest_t0(awaiting: &[Notification]) -> DateTime<Utc> {
        awaiting
            .iter()
            .map(|note| note.occurrence)
            .min()
            .expect("the caller never steps an empty awaiting set")
    }

    /// The next re-toast instant strictly after `last` (or the opener
    /// when no re-toast fired yet) for one occurrence at `t0`.
    fn next_re_toast_after(t0: DateTime<Utc>, last: Option<DateTime<Utc>>) -> DateTime<Utc> {
        match last {
            None => t0 + RE_TOAST_FIRST,
            Some(last) => {
                let elapsed = (last - t0 - RE_TOAST_FIRST).num_seconds().max(0);
                let steps = elapsed / RE_TOAST_EVERY.num_seconds() + 1;
                t0 + RE_TOAST_FIRST + RE_TOAST_EVERY * i32::try_from(steps).unwrap_or(i32::MAX)
            }
        }
    }

    /// The next chime instant strictly after `last` (or the opener),
    /// keyed off the earliest t₀.
    fn next_chime_after(t0min: DateTime<Utc>, last: Option<DateTime<Utc>>) -> DateTime<Utc> {
        match last {
            None => t0min + CHIME_FIRST,
            Some(last) => {
                let elapsed = (last - t0min - CHIME_FIRST).num_seconds().max(0);
                let steps = elapsed / CHIME_EVERY.num_seconds() + 1;
                t0min + CHIME_FIRST + CHIME_EVERY * i32::try_from(steps).unwrap_or(i32::MAX)
            }
        }
    }

    /// The chime volume at instant `c`, ramping [`CHIME_LO`]→[`CHIME_HI`]
    /// over [`CHIME_RAMP`] from the earliest t₀, capped after.
    fn chime_volume(t0min: DateTime<Utc>, c: DateTime<Utc>) -> f32 {
        #[allow(clippy::cast_precision_loss)] // a seconds count on the ramp's scale
        let elapsed = (c - t0min).num_seconds() as f32;
        #[allow(clippy::cast_precision_loss)] // a constant seconds count
        let ramp = CHIME_RAMP.num_seconds() as f32;
        (CHIME_LO + (CHIME_HI - CHIME_LO) * (elapsed / ramp)).clamp(CHIME_LO, CHIME_HI)
    }
}

/// One insistence step: what is due NOW, and when to wake next. Pure:
/// in → (actions, bookkeeping transition, next deadline); no I/O, no
/// clock reads. `memory` is updated as part of the transition (actions
/// are recorded at their actual instants). Derivation rule: an action
/// is due iff its scheduled instant ≤ `now` and no equal-or-later
/// action of the same channel has been recorded — and at most one
/// action per channel per call fires even if several instants piled up
/// behind a descheduling (bounded: one re-toast per occurrence per
/// interval, D-5).
///
/// # Panics
/// Never: an empty awaiting set is the caller's contract to park on
/// (asserted by returning `next_due: None`).
#[must_use]
pub(crate) fn escalation_step(
    awaiting: &[Notification],
    memory: &mut LadderMemory,
    now: DateTime<Utc>,
) -> StepOutcome {
    if awaiting.is_empty() {
        // Bookkeeping dies with the set (D-12); the empty outcome never
        // arms a timer.
        *memory = LadderMemory::default();
        return StepOutcome::default();
    }
    memory.evict(awaiting);
    let t0min = LadderMemory::earliest_t0(awaiting);

    // The re-toast channel: the occurrence whose next scheduled instant
    // is earliest wins (at most one per call — D-5).
    let mut re_toast: Option<(DateTime<Utc>, usize)> = None;
    for (i, note) in awaiting.iter().enumerate() {
        let last = memory
            .last_re_toast
            .get(&(note.reminder, note.occurrence))
            .copied();
        let due = LadderMemory::next_re_toast_after(note.occurrence, last);
        if due <= now && re_toast.as_ref().is_none_or(|(best, _)| due < *best) {
            re_toast = Some((due, i));
        }
    }

    // The chime channel: one global schedule off the earliest t₀.
    let chime_due = LadderMemory::next_chime_after(t0min, memory.last_chime);
    let chime_pending = chime_due <= now;

    let mut actions = Vec::new();
    if let Some((_, i)) = re_toast {
        let note = &awaiting[i];
        memory
            .last_re_toast
            .insert((note.reminder, note.occurrence), now);
        actions.push(EscalationAction::ReToast(note.clone()));
    }
    let mut next_due = None;
    if chime_pending {
        memory.last_chime = Some(now);
        actions.push(EscalationAction::Chime {
            volume: LadderMemory::chime_volume(t0min, now),
        });
    }

    // The next wake: the earliest unrecorded scheduled instant across
    // both channels. Both ladders repeat forever while the set is
    // non-empty, so this is always `Some` here.
    for note in awaiting {
        let last = memory
            .last_re_toast
            .get(&(note.reminder, note.occurrence))
            .copied();
        let due = LadderMemory::next_re_toast_after(note.occurrence, last);
        if next_due.is_none_or(|best| due < best) {
            next_due = Some(due);
        }
    }
    let chime_next = LadderMemory::next_chime_after(t0min, memory.last_chime);
    if next_due.is_none_or(|best| chime_next < best) {
        next_due = Some(chime_next);
    }
    StepOutcome { actions, next_due }
}

// -- the driver task (process glue) -----------------------------------

/// Applies one action at the seams: a re-toast is withdraw-then-deliver
/// (D-3 — replacement, not stacking, at the tracked-handle boundary);
/// both results logged-and-dropped (one lost toast never kills the
/// task). A chime failure is logged-and-dropped (D-8).
async fn apply(sink: &dyn NotificationSink, chime: &dyn ChimeSink, action: EscalationAction) {
    match action {
        EscalationAction::ReToast(note) => {
            if let Err(err) = sink.withdraw(&note.reminder, &note.occurrence).await {
                tracing::debug!(
                    %err,
                    reminder = %note.reminder.text_form(),
                    "escalation: toast withdrawal failed (replacing)"
                );
            }
            if let Err(err) = sink.deliver(&note).await {
                tracing::warn!(
                    %err,
                    reminder = %note.reminder.text_form(),
                    "escalation: re-toast delivery failed"
                );
            }
        }
        EscalationAction::Chime { volume } => {
            if let Err(err) = chime.chime(volume) {
                tracing::warn!(%err, "escalation: chime failed (visual-only)");
            }
        }
    }
}

/// Spawns the escalation driver: parks on the engine-signal broadcast
/// with no armed timer while no `alarm`-insistent notification awaits;
/// wakes on state changes (or its own cadence timer, armed only while
/// an alarm awaits) and steps the pure ladder. Exits when the engine's
/// broadcast closes (D-10); `Lagged` counts as change (the UI's
/// recorded semantics).
#[must_use]
pub(crate) fn spawn_escalation_driver(
    state: Arc<arc_swap::ArcSwap<AppState>>,
    defaults: ReminderDefaults,
    sink: Arc<dyn NotificationSink>,
    chime: Arc<dyn ChimeSink>,
    mut signals: broadcast::Receiver<EngineSignal>,
    clock: Arc<dyn Clock>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!("escalation: driver started");
        let mut memory = LadderMemory::default();
        loop {
            let now = clock.now();
            let awaiting = awaiting_alarms(&state.load(), &defaults, now);
            if awaiting.is_empty() {
                // Bookkeeping dies with the set (D-12); park on the
                // signal lane with NO timer armed.
                memory = LadderMemory::default();
                match signals.recv().await {
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("escalation: signal channel closed, driver exiting");
                        return;
                    }
                }
            }
            let outcome = escalation_step(&awaiting, &mut memory, now);
            for action in outcome.actions {
                apply(sink.as_ref(), chime.as_ref(), action).await;
            }
            // The ladder repeats forever while the set is non-empty;
            // `None` cannot occur here (an empty set parks above).
            let Some(next_due) = outcome.next_due else {
                continue;
            };
            tokio::select! {
                () = tokio::time::sleep((next_due - now).to_std().unwrap_or_default()) => {}
                r = signals.recv() => match r {
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("escalation: signal channel closed, driver exiting");
                        return;
                    }
                },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    const T0: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn alarm_note(label: &str, occurrence: DateTime<Utc>) -> Notification {
        let alarm = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        Notification {
            insistence: Insistence::Alarm,
            reminder: ReminderRef::Adhoc(alarm),
            anchor: taskboard_domain::ReminderAnchor::Alarm(alarm),
            title: label.to_owned(),
            occurrence,
        }
    }

    // -- M1: the pure ladder ------------------------------------------

    #[test]
    fn first_re_toast_opens_one_minute_after_the_occurrence() {
        let mut memory = LadderMemory::default();
        let notes = [alarm_note("tea", T0)];
        let early = escalation_step(&notes, &mut memory, T0 + TimeDelta::seconds(59));
        assert!(early.actions.is_empty(), "no action before the opener");
        assert_eq!(early.next_due, Some(T0 + RE_TOAST_FIRST));
        let at = escalation_step(&notes, &mut memory, T0 + RE_TOAST_FIRST);
        // Both insistence channels open at +1 min (the review fence),
        // so the chime rides — the re-toast is the asserted channel.
        assert_eq!(
            at.actions
                .iter()
                .filter(|a| matches!(a, EscalationAction::ReToast(_)))
                .count(),
            1
        );
        assert!(matches!(&at.actions[0], EscalationAction::ReToast(n) if n.occurrence == T0));
    }

    #[test]
    fn re_toasts_hold_the_two_minute_cadence() {
        let mut memory = LadderMemory::default();
        let notes = [alarm_note("tea", T0)];
        // The opener is consumed at +1m; the walk below checks the
        // two-minute cadence that follows it.
        let _ = escalation_step(&notes, &mut memory, T0 + RE_TOAST_FIRST);
        for offset in [3, 5, 7] {
            let step = escalation_step(
                &notes,
                &mut memory,
                T0 + TimeDelta::minutes(offset) - TimeDelta::seconds(1),
            );
            assert_eq!(
                step.actions
                    .iter()
                    .filter(|a| matches!(a, EscalationAction::ReToast(_)))
                    .count(),
                0,
                "no re-toast a second early at +{offset}m (the chime grid is its own channel)"
            );
            let at = escalation_step(&notes, &mut memory, T0 + TimeDelta::minutes(offset));
            assert_eq!(
                at.actions
                    .iter()
                    .filter(|a| matches!(a, EscalationAction::ReToast(_)))
                    .count(),
                1,
                "exactly one re-toast at t₀+{offset}m"
            );
        }
        // The memory transition: a second step at the same instant is
        // silent.
        let again = escalation_step(&notes, &mut memory, T0 + TimeDelta::minutes(7));
        assert!(
            again.actions.is_empty(),
            "the recorded instant deduplicates"
        );
    }

    #[test]
    fn first_chime_opens_one_minute_after_the_earliest_occurrence() {
        let mut memory = LadderMemory::default();
        // Two awaiting occurrences 10 s apart — the global channel keys
        // off the earlier t₀ and one chime serves both (D-4).
        let notes = [
            alarm_note("a", T0),
            alarm_note("b", T0 + TimeDelta::seconds(10)),
        ];
        let early = escalation_step(&notes, &mut memory, T0 + TimeDelta::seconds(59));
        assert_eq!(early.actions, [] as [EscalationAction; 0]);
        let at = escalation_step(&notes, &mut memory, T0 + CHIME_FIRST);
        assert_eq!(
            at.actions
                .iter()
                .filter(|a| matches!(a, EscalationAction::Chime { .. }))
                .count(),
            1,
            "one chime serves both occurrences"
        );
        // ...and the re-toast of the earlier occurrence rides along (its
        // opener coincides with the chime opener).
        assert!(
            at.actions
                .iter()
                .any(|a| matches!(a, EscalationAction::ReToast(_)))
        );
    }

    #[test]
    fn chimes_repeat_every_thirty_seconds() {
        let mut memory = LadderMemory::default();
        let notes = [alarm_note("tea", T0)];
        // The opener (shared with the re-toast) consumes the +1m chime;
        // the next lands at +1m30s.
        let _ = escalation_step(&notes, &mut memory, T0 + CHIME_FIRST);
        let step = escalation_step(
            &notes,
            &mut memory,
            T0 + CHIME_FIRST + CHIME_EVERY - TimeDelta::seconds(1),
        );
        assert!(
            !step
                .actions
                .iter()
                .any(|a| matches!(a, EscalationAction::Chime { .. }))
        );
        let at = escalation_step(&notes, &mut memory, T0 + CHIME_FIRST + CHIME_EVERY);
        assert_eq!(
            at.actions
                .iter()
                .filter(|a| matches!(a, EscalationAction::Chime { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn volume_ramps_thirty_to_hundred_percent_over_five_minutes_capped() {
        let t0min = T0;
        for (offset, lo, hi) in [
            (TimeDelta::minutes(1), 0.43, 0.45),
            (TimeDelta::minutes(3), 0.71, 0.73),
            (TimeDelta::minutes(4) + TimeDelta::seconds(50), 0.97, 0.985),
            (TimeDelta::minutes(6), 1.0, 1.0),
        ] {
            let volume = LadderMemory::chime_volume(t0min, t0min + offset);
            assert!(
                volume >= lo - f32::EPSILON && volume <= hi + f32::EPSILON,
                "volume at +{offset:?}: {volume}"
            );
            if offset < CHIME_RAMP {
                assert!(volume < CHIME_HI, "still ramping at +{offset:?}");
            } else {
                assert!((volume - CHIME_HI).abs() < 1e-6, "capped at +{offset:?}");
            }
        }
    }

    #[test]
    fn a_descheduled_driver_never_bursts() {
        let mut memory = LadderMemory::default();
        let notes = [alarm_note("tea", T0)];
        // Nothing steps between t₀+1m and t₀+4m; one step then fires
        // exactly one re-toast + one chime (D-5) despite three due
        // re-toast instants and five chime instants.
        let step = escalation_step(&notes, &mut memory, T0 + TimeDelta::minutes(4));
        assert_eq!(step.actions.len(), 2);
        assert!(
            step.actions
                .iter()
                .any(|a| matches!(a, EscalationAction::ReToast(_)))
        );
        assert!(
            step.actions
                .iter()
                .any(|a| matches!(a, EscalationAction::Chime { .. }))
        );
        // The next wake lands on a real boundary, not in the past.
        assert!(
            step.next_due
                .is_some_and(|due| due > T0 + TimeDelta::minutes(4))
        );
    }

    #[test]
    fn memory_dies_with_the_awaiting_set() {
        let a = alarm_note("a", T0);
        let b = alarm_note("b", T0 + TimeDelta::seconds(10));
        let mut memory = LadderMemory::default();
        let both = [a.clone(), b.clone()];
        // One action per channel per call: the earlier occurrence's
        // opener lands first, the later one's ten seconds behind it.
        let _ = escalation_step(&both, &mut memory, T0 + TimeDelta::minutes(1));
        assert_eq!(memory.last_re_toast.len(), 1);
        let _ = escalation_step(
            &both,
            &mut memory,
            T0 + TimeDelta::minutes(1) + TimeDelta::seconds(10),
        );
        assert_eq!(memory.last_re_toast.len(), 2);

        // Dismiss one: its re-toast schedule stops (eviction), the
        // other continues.
        let only_b = [b.clone()];
        let step = escalation_step(
            &only_b,
            &mut memory,
            T0 + TimeDelta::minutes(3) + TimeDelta::seconds(10),
        );
        assert_eq!(
            step.actions
                .iter()
                .filter(|a| matches!(a, EscalationAction::ReToast(_)))
                .count(),
            1,
            "the surviving occurrence keeps its cadence"
        );
        assert_eq!(
            memory.last_re_toast.len(),
            1,
            "the dismissed key was evicted"
        );

        // Empty set: the whole memory dies.
        let _ = escalation_step(&[], &mut memory, T0 + TimeDelta::minutes(4));
        assert_eq!(memory.last_re_toast.len(), 0);
        assert_eq!(memory.last_chime, None);
    }

    // -- M3: synthesis (structural, no audio device) ------------------

    #[test]
    fn chime_samples_are_structurally_sound() {
        let samples = chime_samples();
        assert_eq!(
            samples.len(),
            CHIME_TOTAL_SAMPLES,
            "length = the named constant"
        );
        let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "peak {peak} exceeds full scale");
        assert!(peak > 0.1, "the waveform is not silent");
        // The raised-cosine fades: both edges start/end near silence.
        assert!(samples[0].abs() < 0.01, "the fade-in starts silent");
        assert!(
            samples[samples.len() - 1].abs() < 0.01,
            "the fade-out ends silent"
        );
        // The two tones dominate the two halves: the mean square of each
        // half is well above a silent floor.
        for half in [&samples[..samples.len() / 2], &samples[samples.len() / 2..]] {
            let energy: f32 = half.iter().map(|s| s * s).sum();
            #[allow(clippy::cast_precision_loss)] // a mean over 13k samples
            let mean = energy / half.len() as f32;
            assert!(mean > 0.001, "the half carries a tone");
        }
    }

    // -- M3: the audio seam -------------------------------------------

    #[test]
    fn audio_init_failure_degrades_to_visual_only() {
        // The failing-open seam: the typed NoDevice class mints the
        // degrade path; the disabled sink is a typed no-op afterwards.
        let chime = mount_chime(true, || {
            Err(AudioError::Open(
                rodio::stream::DeviceSinkError::NoDevice.to_string(),
            ))
        });
        assert!(
            chime.chime(0.5).is_ok(),
            "the disabled sink never fails the caller"
        );
        // The gate: not (Desktop ∧ Desktop-sink) never even opens.
        let gated = mount_chime(false, || unreachable!("the gate skips construction"));
        assert!(gated.chime(1.0).is_ok());
    }

    // -- M2: the awaiting predicate + the driver loop -----------------

    fn awaiting_notification_state() -> (AppState, taskboard_domain::AdhocAlarmId) {
        let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.reminders.alarms.insert(
            id,
            taskboard_domain::AdhocAlarm {
                insistence: Insistence::Alarm,
                id,
                label: "tea".to_owned(),
                fire_at: T0,
                created_at: T0 - TimeDelta::hours(1),
            },
        );
        state
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: ReminderRef::Adhoc(id),
                occurrence: T0,
                delivered_at: T0,
            });
        (state, id)
    }

    fn defaults() -> ReminderDefaults {
        ReminderDefaults::default()
    }

    /// The fake clock: a nanos-timestamp cell the test drives.
    #[derive(Debug, Default)]
    struct FakeClock(AtomicI64);

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            chrono::DateTime::<Utc>::from_timestamp(self.0.load(Ordering::SeqCst), 0)
                .expect("valid fake instant")
        }
    }

    impl FakeClock {
        fn set(&self, t: DateTime<Utc>) {
            self.0.store(t.timestamp(), Ordering::SeqCst);
        }
    }

    /// The recording sink: ordered `(deliver | withdraw, reminder,
    /// occurrence)` records (the `CollectingSink` pattern).
    #[derive(Default)]
    struct FakeSink {
        records: std::sync::Mutex<Vec<(&'static str, ReminderRef, DateTime<Utc>)>>,
        fail_deliver: std::sync::atomic::AtomicBool,
    }

    impl std::fmt::Debug for FakeSink {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("FakeSink")
        }
    }

    impl NotificationSink for FakeSink {
        fn deliver<'a>(
            &'a self,
            notification: &'a Notification,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(), crate::notifications::NotifyError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.records.lock().expect("sink records").push((
                    "deliver",
                    notification.reminder,
                    notification.occurrence,
                ));
                if self.fail_deliver.load(Ordering::SeqCst) {
                    return Err(crate::notifications::NotifyError);
                }
                Ok(())
            })
        }

        fn withdraw<'a>(
            &'a self,
            reminder: &'a ReminderRef,
            occurrence: &'a DateTime<Utc>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(), crate::notifications::NotifyError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.records.lock().expect("sink records").push((
                    "withdraw",
                    *reminder,
                    *occurrence,
                ));
                Ok(())
            })
        }
    }

    #[test]
    fn awaiting_alarms_filters_the_domain_view_to_alarm() {
        let (mut state, id) = awaiting_notification_state();
        // A normal-insistence alarm alongside: filtered out.
        let normal = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        state.reminders.alarms.insert(
            normal,
            taskboard_domain::AdhocAlarm {
                insistence: Insistence::Normal,
                id: normal,
                label: "quiet".to_owned(),
                fire_at: T0,
                created_at: T0 - TimeDelta::hours(1),
            },
        );
        state
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: ReminderRef::Adhoc(normal),
                occurrence: T0,
                delivered_at: T0,
            });
        let awaiting = awaiting_alarms(&state, &defaults(), T0 + TimeDelta::minutes(1));
        assert_eq!(awaiting.len(), 1, "only the alarm-insistent row survives");
        assert_eq!(awaiting[0].insistence, Insistence::Alarm);
        assert_eq!(awaiting[0].reminder, ReminderRef::Adhoc(id));
    }

    #[tokio::test(start_paused = true)]
    async fn re_toast_is_withdraw_then_deliver() {
        // D-3 pinned at the lowest layer: one ReToast's apply records
        // exactly [Withdraw, Deliver] for the same key.
        let sink = Arc::new(FakeSink::default());
        let notes = [alarm_note("tea", T0)];
        let mut memory = LadderMemory::default();
        let outcome = escalation_step(&notes, &mut memory, T0 + TimeDelta::minutes(1));
        let action = outcome
            .actions
            .iter()
            .find_map(|a| match a {
                EscalationAction::ReToast(n) => Some(n.clone()),
                EscalationAction::Chime { .. } => None,
            })
            .expect("a re-toast is due at the opener");
        apply(
            sink.as_ref(),
            &DisabledChime,
            EscalationAction::ReToast(action),
        )
        .await;
        let records = sink.records.lock().expect("records").clone();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, "withdraw");
        assert_eq!(records[1].0, "deliver");
        assert_eq!(
            records[0].1, records[1].1,
            "the same (reminder, occurrence) key"
        );
        assert_eq!(records[0].2, records[1].2);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_delivery_does_not_kill_the_driver() {
        // The D-R13 consumer discipline, one layer up: both cadence
        // actions still flow after the first delivery errs.
        let sink = Arc::new(FakeSink {
            fail_deliver: std::sync::atomic::AtomicBool::new(true),
            ..FakeSink::default()
        });
        let notes = [alarm_note("tea", T0)];
        let mut memory = LadderMemory::default();
        let outcome = escalation_step(&notes, &mut memory, T0 + TimeDelta::minutes(1));
        for action in outcome.actions {
            apply(sink.as_ref(), &DisabledChime, action).await;
        }
        let records = sink.records.lock().expect("records").clone();
        assert_eq!(records.len(), 2, "withdraw + deliver both attempted");
        assert_eq!(records[1].0, "deliver");
    }

    #[tokio::test(start_paused = true)]
    async fn the_driver_wakes_only_while_an_alarm_awaits() {
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(AppState::default()));
        let (signals_tx, _keepalive) = broadcast::channel::<EngineSignal>(8);
        let sink = Arc::new(FakeSink::default());
        let clock = Arc::new(FakeClock::default());
        clock.set(T0);
        let driver = spawn_escalation_driver(
            Arc::clone(&state),
            defaults(),
            Arc::clone(&sink) as Arc<dyn NotificationSink>,
            disabled_chime(),
            signals_tx.subscribe(),
            clock.clone(),
        );

        // Idle: no awaiting alarm — advance 10 min of tokio time; the
        // parked driver never wakes, zero sink calls.
        clock.set(T0 + TimeDelta::minutes(10));
        tokio::time::advance(std::time::Duration::from_secs(600)).await;
        tokio::task::yield_now().await;
        assert!(
            sink.records.lock().expect("records").is_empty(),
            "parked while idle"
        );

        // Swap in an awaiting alarm state + signal: the ladder arms
        // (the fake clock rewinds to t₀ so the opener is a clean +1m
        // cadence from the arm).
        clock.set(T0);
        let (armed, _) = awaiting_notification_state();
        state.store(Arc::new(armed));
        signals_tx.send(EngineSignal::StateUpdated).expect("signal");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Advance to the opener: the re-toast lands (the chime rides
        // the disabled sink — the re-toast is the observable).
        clock.set(T0 + TimeDelta::minutes(1));
        tokio::time::advance(std::time::Duration::from_secs(60)).await;
        tokio::task::yield_now().await;
        let records = sink.records.lock().expect("records").clone();
        assert!(
            !records.is_empty(),
            "the re-toast landed after the opener: {records:?}"
        );
        assert!(
            records.iter().any(|(kind, _, _)| *kind == "deliver"),
            "a deliver among the records: {records:?}"
        );

        // Dismiss (swap the state back to empty) + signal: advancing
        // further produces nothing.
        state.store(Arc::new(AppState::default()));
        signals_tx.send(EngineSignal::StateUpdated).expect("signal");
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        clock.set(T0 + TimeDelta::minutes(5));
        let before = sink.records.lock().expect("records").len();
        tokio::time::advance(std::time::Duration::from_secs(300)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            sink.records.lock().expect("records").len(),
            before,
            "the dismissed occurrence's cadence died with its membership"
        );

        // Shut down: dropping the sender closes the broadcast; the
        // driver exits on Closed.
        driver.abort();
        let _ = driver.await;
    }

    #[tokio::test(start_paused = true)]
    async fn normal_insistence_never_escalates() {
        let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut normal_state = AppState::default();
        normal_state.reminders.alarms.insert(
            id,
            taskboard_domain::AdhocAlarm {
                insistence: Insistence::Normal,
                id,
                label: "quiet".to_owned(),
                fire_at: T0,
                created_at: T0 - TimeDelta::hours(1),
            },
        );
        normal_state
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: ReminderRef::Adhoc(id),
                occurrence: T0,
                delivered_at: T0,
            });
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(normal_state));
        let (_, rx) = broadcast::channel::<EngineSignal>(8);
        let sink = Arc::new(FakeSink::default());
        let clock = Arc::new(FakeClock::default());
        clock.set(T0 + TimeDelta::minutes(30));
        let driver = spawn_escalation_driver(
            state,
            defaults(),
            Arc::clone(&sink) as Arc<dyn NotificationSink>,
            disabled_chime(),
            rx,
            clock.clone(),
        );
        tokio::time::advance(std::time::Duration::from_mins(30)).await;
        tokio::task::yield_now().await;
        assert!(
            sink.records.lock().expect("records").is_empty(),
            "a normal-insistence awaiting notification never escalates"
        );
        driver.abort();
        let _ = driver.await;
    }

    #[tokio::test]
    async fn the_driver_exits_when_the_broadcast_closes() {
        let (_, rx) = broadcast::channel::<EngineSignal>(8);
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(AppState::default()));
        let clock = Arc::new(FakeClock::default());
        let driver = spawn_escalation_driver(
            state,
            defaults(),
            Arc::new(crate::notifications::LogSink) as Arc<dyn NotificationSink>,
            disabled_chime(),
            rx,
            clock,
        );
        // The only sender died at the channel's creation binding: the
        // broadcast is already closed, so the driver's first park
        // answers `Closed` and the task exits.
        tokio::time::timeout(std::time::Duration::from_secs(1), driver)
            .await
            .expect("the driver exits on Closed")
            .expect("clean exit");
    }
}
