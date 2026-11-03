// SPDX-License-Identifier: AGPL-3.0-only

//! The wake-interruptible masked-read core (unix).
//!
//! Root cause of #572: `rpassword`'s masked read blocks in `read(2)` on
//! the tty and observes only input. A `^C` byte the kernel line
//! discipline consumes while the terminal is still cooked — the window
//! between rpassword's label write and its `tcsetattr` — raises SIGINT
//! (which arms the session watcher's cancel epoch and prints the
//! `cancelled` envelope) but leaves nothing for the reader: the editor
//! thread parks in `read(2)` forever, `readline` never re-arms, and the
//! pty goes silent (the wedge). Under load that sub-millisecond window
//! stretches to scheduler timescales — the ~1/25 loaded rate.
//!
//! The cure: the masked read polls a second, cancellable source — a
//! wake socket written by the session loop's cancel arm while a prompt
//! disposition is in flight — so a cancellation always unblocks the
//! read. ADR 0014 §6's handshake row ("answering prompt … woke by the
//! user's input") is satisfied even when the keystroke that woke the
//! session never reached the prompt.
//!
//! Byte grammar parity with rpassword 7.5.4 (vendored `lib.rs:88-191`,
//! `feedback.rs`, `utf8.rs`), the parity table:
//!
//! | input | behavior |
//! |---|---|
//! | `\n` / `\r` | finish: newline to the tty, `Ok(password)` |
//! | DEL / backspace | pop one char, `\x08 \x08` per displayed mask |
//! | `^U` | clear all, `\x08 \x08` per displayed mask |
//! | `^W` | clear to the last space, masks popped accordingly |
//! | `^C` | abort newline, `raise(SIGINT)` (the Q11 duality: the same
//!   keystroke races the loop's typed `cancelled` against the prompt's
//!   typed `Interrupted`), `Err(Interrupted)` |
//! | `^D` | empty password → `Err(UnexpectedEof)`; non-empty → ignored |
//! | ESC | consume a CSI (`ESC [`) / SS3 (`ESC O`) sequence whole |
//! | printable | push the char, echo one `*` mask |
//! | other control bytes | discarded |
//! | UTF-8 lead byte | continuation bytes assembled; truncated or
//!   invalid → U+FFFD |
//! | EOF (read 0) | finish quietly: `Ok(password so far)` |
//!
//! Deliberate deviations from rpassword (each load-bearing):
//! - a wake is always a typed `Err(Interrupted)` with the termios
//!   restored and no input consumed — even mid-character or
//!   mid-escape-sequence (rpassword has no wake concept);
//! - a wake and a tty byte ready together process the byte first and
//!   take the wake next — no lost wake (the byte is already consumed
//!   data; the cancellation is still delivered).

use std::io::IsTerminal;
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{Signal, raise};
use nix::sys::termios::{
    LocalFlags, SetArg, SpecialCharacterIndices as cc, Termios, tcgetattr, tcsetattr,
};
use nix::unistd::read;

/// The write half of the wake channel: the session loop's cancel arm
/// fires this while a prompt disposition is outstanding.
#[derive(Debug)]
pub struct WakeSink(UnixStream);

/// The read half, owned by the editor thread and polled by the
/// masked-read core.
#[derive(Debug)]
pub struct WakeSource(UnixStream);

/// Builds the wake channel: a connected, nonblocking socket pair. A
/// one-byte write never blocks (the kernel buffers it), so the async
/// cancel arm can fire without parking.
///
/// # Errors
///
/// [`std::io::Error`] when the socket pair cannot be created.
pub fn wakeup_channel() -> io::Result<(WakeSink, WakeSource)> {
    let (sink, source) = UnixStream::pair()?;
    sink.set_nonblocking(true)?;
    source.set_nonblocking(true)?;
    Ok((WakeSink(sink), WakeSource(source)))
}

impl WakeSink {
    /// Signals every armed masked read. Idempotent by construction: a
    /// one-byte write, dropped when the buffer is full — a full buffer
    /// means a wake is already pending.
    pub(crate) fn fire(&self) {
        let _ = nix::unistd::write(&self.0, &[1]);
    }
}

impl std::os::fd::AsFd for WakeSink {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl std::os::fd::AsFd for WakeSource {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Clone for WakeSink {
    fn clone(&self) -> Self {
        WakeSink(self.0.try_clone().expect("clone the wake sink"))
    }
}

impl WakeSource {
    /// Drains and counts the readable wake bytes — the session-tier
    /// pins observe fires through this (0 = none pending).
    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        let mut total = 0;
        loop {
            match read(&self.0, &mut [0u8; 16]) {
                Ok(n) if n > 0 => total += n,
                _ => break,
            }
        }
        total
    }

    /// Consumes every pending wake byte (the stale-wake discipline,
    /// plan D5): a wake racing an already-finishing prompt must never
    /// abandon a later one.
    pub(crate) fn drain(&self) {
        loop {
            match read(&self.0, &mut [0u8; 64]) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }
}

/// One input byte, or the event that supersedes it.
enum Byte {
    One(u8),
    Eof,
    Wake,
}

/// Waits for the next byte on `input`, unless the wake fires first. A
/// wake ready alongside an already-readable byte is *not* taken here —
/// the byte wins this round and the wake is taken on the next one.
fn wait_byte(input: BorrowedFd<'_>, wake: Option<&WakeSource>) -> io::Result<Byte> {
    loop {
        let mut fds = Vec::with_capacity(2);
        fds.push(PollFd::new(input, PollFlags::POLLIN));
        if let Some(wake) = wake {
            fds.push(PollFd::new(wake.0.as_fd(), PollFlags::POLLIN));
        }
        match poll(&mut fds, PollTimeout::NONE) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(err) => return Err(io::Error::from(err)),
        }
        let input_ready = fds[0]
            .revents()
            .is_some_and(|events| events.contains(PollFlags::POLLIN));
        let wake_ready = wake.is_some()
            && fds[1]
                .revents()
                .is_some_and(|events| events.contains(PollFlags::POLLIN));
        if !input_ready {
            if wake_ready {
                if let Some(wake) = wake {
                    wake.drain();
                }
                return Ok(Byte::Wake);
            }
            continue;
        }
        let mut buf = [0u8; 1];
        return match read(input, &mut buf) {
            Ok(1) => Ok(Byte::One(buf[0])),
            Ok(_) => Ok(Byte::Eof),
            Err(nix::errno::Errno::EINTR) => continue,
            Err(err) => Err(io::Error::from(err)),
        };
    }
}

/// Reads one rpassword-grammar char: a byte, a UTF-8 assembly, or the
/// typed error the grammar table maps (mirrors vendored `utf8.rs`,
/// with every blocking byte read poll-gated against the wake).
fn read_char(input: BorrowedFd<'_>, wake: Option<&WakeSource>) -> io::Result<char> {
    let byte = match wait_byte(input, wake)? {
        Byte::One(byte) => byte,
        Byte::Eof => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof")),
        Byte::Wake => return Err(io::Error::from(io::ErrorKind::Interrupted)),
    };
    match byte {
        // ASCII
        0x00..=0x7F => Ok(byte as char),
        // UTF-8 lead byte
        0xC2..=0xF4 => {
            let width = match byte {
                0xC2..=0xDF => 2,
                0xE0..=0xEF => 3,
                _ => 4,
            };
            let mut buf = vec![byte];
            for _ in 1..width {
                match wait_byte(input, wake)? {
                    Byte::One(byte) => buf.push(byte),
                    // Truncated multibyte char: U+FFFD parity.
                    Byte::Eof => return Ok('\u{FFFD}'),
                    // A wake mid-char is still a cancellation: the
                    // typed interruption wins over FFFD parity.
                    Byte::Wake => return Err(io::Error::from(io::ErrorKind::Interrupted)),
                }
            }
            Ok(std::str::from_utf8(&buf)
                .ok()
                .and_then(|s| s.chars().next())
                .unwrap_or('\u{FFFD}'))
        }
        // Invalid byte
        _ => Ok('\u{FFFD}'),
    }
}

/// The mask-feedback state (vendored `feedback.rs` parity, with
/// `needs_terminal_configuration` fixed true — this core only runs on
/// a tty): the password and the number of masks currently displayed.
#[derive(Default)]
struct MaskState {
    password: String,
    displayed: usize,
}

impl MaskState {
    fn push(&mut self, c: char) -> String {
        self.password.push(c);
        self.displayed += 1;
        "*".to_owned()
    }

    fn pop(&mut self) -> String {
        match self.password.pop() {
            Some(_) => {
                if self.displayed > 0 {
                    self.displayed -= 1;
                    "\x08 \x08".to_owned()
                } else {
                    String::new()
                }
            }
            None => String::new(),
        }
    }

    fn clear(&mut self) -> String {
        self.password.clear();
        let count = self.displayed;
        self.displayed = 0;
        "\x08 \x08".repeat(count)
    }

    fn clear_til_last_space(&mut self) -> String {
        let mut trimmed = self.password.trim_end();
        trimmed = match trimmed.rfind(' ') {
            Some(last_space) => &trimmed[..=last_space],
            None => "",
        };
        let new_displayed = trimmed.chars().count();
        let removed = self.password.chars().count() - new_displayed;
        self.password = trimmed.to_owned();
        let count = self.displayed;
        self.displayed = new_displayed;
        "\x08 \x08".repeat(removed.min(count))
    }

    fn abort(&mut self) -> &'static str {
        self.password.clear();
        self.displayed = 0;
        "\n"
    }
}

/// Restores the saved termios on every exit path (vendored
/// `RawModeInput::drop` parity) — the guard makes a dropped restore
/// impossible.
struct RawMode<'a> {
    fd: BorrowedFd<'a>,
    saved: Termios,
}

impl<'a> RawMode<'a> {
    /// Saves the line discipline and applies the masked-read config:
    /// `ECHO | ICANON | ECHONL | ISIG` off (the `^C` byte becomes
    /// data), `VMIN=1`, `TCSANOW` (vendored
    /// `apply_terminal_configuration` parity).
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the termios cannot be read or applied.
    fn new(fd: BorrowedFd<'a>) -> io::Result<Self> {
        let saved = tcgetattr(fd).map_err(io::Error::from)?;
        let mut raw = saved.clone();
        raw.local_flags
            .remove(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ECHONL | LocalFlags::ISIG);
        raw.control_chars[cc::VMIN as usize] = 1;
        raw.control_chars[cc::VTIME as usize] = 0;
        tcsetattr(fd, SetArg::TCSANOW, &raw).map_err(io::Error::from)?;
        Ok(Self { fd, saved })
    }
}

impl Drop for RawMode<'_> {
    fn drop(&mut self) {
        let _ = tcsetattr(self.fd, SetArg::TCSANOW, &self.saved);
    }
}

/// The masked-read core over injected descriptors: one raw-mode,
/// poll-gated read with full rpassword 7.5.4 grammar parity (the
/// module docs carry the table). Termios are applied before the first
/// read and restored on every exit path.
///
/// The core performs no stale-wake drain of its own: the caller owns
/// the discipline (at prompt start, before arming).
///
/// # Errors
///
/// [`std::io::Error`]: `Interrupted` on `^C` or a wake,
/// `UnexpectedEof` on `^D` at an empty password; otherwise the read's
/// io error.
pub(crate) fn read_masked(
    input: BorrowedFd<'_>,
    feedback: &mut dyn Write,
    wake: Option<&WakeSource>,
) -> io::Result<String> {
    let _guard = RawMode::new(input)?;
    let mut state = MaskState::default();
    loop {
        let c = match read_char(input, wake) {
            Ok(c) => c,
            // EOF parity: finish quietly with the password so far.
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err),
        };
        match c {
            // LF / CR (Enter)
            '\n' | '\r' => {
                // Finish: one newline closes the prompt line
                // (vendored `finish` parity; needs_terminal is always
                // true on this path).
                feedback.write_all(b"\n")?;
                break;
            }
            // Backspace / DEL
            '\x7F' | '\x08' => {
                let output = state.pop();
                feedback.write_all(output.as_bytes())?;
            }
            // Ctrl-U: clear line
            '\x15' => {
                let output = state.clear();
                feedback.write_all(output.as_bytes())?;
            }
            // Ctrl-W: clear to last space
            '\x17' => {
                let output = state.clear_til_last_space();
                feedback.write_all(output.as_bytes())?;
            }
            // Ctrl-C: interrupt
            '\x03' => {
                let output = state.abort();
                feedback.write_all(output.as_bytes())?;
                // The Q11 raise: the same keystroke that returns the
                // prompt's typed `Interrupted` also arms the session
                // watcher's cancel epoch, racing the loop's typed
                // `cancelled` — the duality the pty pins assert.
                raise(Signal::SIGINT).map_err(io::Error::from)?;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            // Ctrl-D: EOF when empty
            '\x04' => {
                if state.password.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected end of file",
                    ));
                }
            }
            // ESC: consume and discard escape sequences like arrow keys
            '\x1B' => {
                let c = match read_char(input, wake) {
                    Ok(c) => c,
                    Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
                    Err(err) => return Err(err),
                };
                if c == '[' || c == 'O' {
                    // CSI (ESC [) or SS3 (ESC O): read until the final
                    // byte (0x40-0x7E).
                    loop {
                        let c = match read_char(input, wake) {
                            Ok(c) => c,
                            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                                feedback.write_all(b"\n")?;
                                return Ok(state.password);
                            }
                            Err(err) => return Err(err),
                        };
                        if ('\x40'..='\x7E').contains(&c) {
                            break;
                        }
                    }
                }
            }
            c if !c.is_control() => {
                let output = state.push(c);
                feedback.write_all(output.as_bytes())?;
            }
            // Discard unrecognized control characters
            _ => {}
        }
    }
    Ok(state.password)
}

/// The editor's production masked read: the stale-wake drain (prompt
/// start), the label on `/dev/tty`, then the core. On a non-terminal
/// stdin the piped fallback (`StdinPrompts::read_secret_sync`'s cooked
/// branch) applies, verbatim — the core only runs on a tty.
///
/// # Errors
///
/// [`std::io::Error`] as [`read_masked`], or when `/dev/tty` cannot be
/// opened.
pub fn read_secret_tty(label: &str, wake: Option<&WakeSource>) -> io::Result<String> {
    if !std::io::stdin().is_terminal() {
        return crate::commands::connect::StdinPrompts::read_secret_sync(label);
    }
    // Stale-wake discipline (D5): a wake that raced the previous
    // prompt's finish must never abandon this one.
    if let Some(wake) = wake {
        wake.drain();
    }
    let tty_in = std::fs::File::open("/dev/tty")?;
    let mut tty_out = std::fs::OpenOptions::new().write(true).open("/dev/tty")?;
    tty_out.write_all(label.as_bytes())?;
    tty_out.flush()?;
    let password = read_masked(tty_in.as_fd(), &mut tty_out, wake)?;
    Ok(password.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::{MaskState, WakeSource, read_masked, wakeup_channel};
    use nix::pty::{OpenptyResult, openpty};
    use nix::sys::termios::{Termios, tcgetattr};
    use std::os::fd::{AsFd, OwnedFd};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn feed(master: &OwnedFd, bytes: &[u8]) {
        nix::unistd::write(master, bytes).expect("feed the pty");
    }

    fn pty() -> (OwnedFd, OwnedFd) {
        let OpenptyResult { master, slave } = openpty(None, None).expect("openpty");
        (master, slave)
    }

    /// No sleeps: the test observes the core arming the raw mode (the
    /// ISIG flip) by polling the termios, and only then feeds — the
    /// kernel line discipline must never see the test's bytes cooked
    /// (it would eat the control bytes: the very wedge under repair).
    fn wait_armed(slave: &OwnedFd, saved: &Termios) {
        for _ in 0..50_000_000 {
            let cur = tcgetattr(slave.as_fd()).expect("probe the termios");
            if cur.local_flags != saved.local_flags {
                return;
            }
        }
        panic!("the core never armed the raw mode");
    }

    /// Runs the core on the pty slave (a blocking read needs a thread
    /// of its own); `arrange` fires once the raw mode is armed — fed
    /// input reaches the core's grammar, a fired wake reaches its poll.
    fn run_with(
        slave: &OwnedFd,
        wake: Option<&WakeSource>,
        arrange: impl FnOnce(),
    ) -> (std::io::Result<String>, Vec<u8>) {
        let saved = tcgetattr(slave.as_fd()).expect("save the termios");
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let mut feedback = Vec::new();
                let result = read_masked(slave.as_fd(), &mut feedback, wake);
                (result, feedback)
            });
            wait_armed(slave, &saved);
            arrange();
            handle.join().expect("the core thread joins")
        })
    }

    /// One grammar-table row: bytes fed into the armed pty → typed
    /// outcome + password + feedback bytes (the parity table,
    /// executed).
    fn grammar_row(input: &[u8]) -> (Result<std::io::ErrorKind, String>, Vec<u8>) {
        let (master, slave) = pty();
        let (outcome, feedback) = run_with(&slave, None, || {
            feed(&master, input);
        });
        let typed = match outcome {
            Ok(password) => Err(password),
            Err(err) => Ok(err.kind()),
        };
        (typed, feedback)
    }

    /// The grammar table's Ok rows (the `^C` and `^D`-empty rows are
    /// typed, so they pin separately below).
    #[test]
    fn the_grammar_table_finishes_pops_and_clears_like_rpassword() {
        let rows: Vec<(&[u8], &str, &[u8])> = vec![
            // Enter (LF and CR variants)
            (b"ab\n", "ab", b"**\n"),
            (b"ab\r", "ab", b"**\n"),
            // Backspace pops one displayed mask per erased char
            (b"abc\x7f\n", "ab", b"***\x08 \x08\n"),
            // DEL on an empty password is silent
            (b"\x7f\n", "", b"\n"),
            // ^U clears all; ^W clears to the last space
            (b"abc\x15\n", "", b"***\x08 \x08\x08 \x08\x08 \x08\n"),
            (b"ab cd\x17\n", "ab ", b"*****\x08 \x08\x08 \x08\n"),
            // Unrecognized control bytes are discarded
            (b"a\x01b\n", "ab", b"**\n"),
            // ^D on a non-empty password is ignored
            (b"ab\x04\n", "ab", b"**\n"),
            // ESC sequences are consumed whole (CSI and SS3): the
            // final byte is part of the sequence — "a ESC [ B" then b
            // edits to "ab", while "a ESC O b" consumes the b as the
            // SS3 final byte (vendored lib.rs parity)
            (b"a\x1b[Bb\n", "ab", b"**\n"),
            (b"a\x1bOb\n", "a", b"*\n"),
            // UTF-8: one mask per char, whatever the byte width; an
            // invalid byte surfaces as U+FFFD
            ("é\n".as_bytes(), "é", b"*\n"),
            ("🚲\n".as_bytes(), "🚲", b"*\n"),
            (b"\xff\n", "\u{FFFD}", b"*\n"),
        ];
        for (input, password, feedback) in rows {
            let (outcome, output) = grammar_row(input);
            assert_eq!(outcome.unwrap_err(), password, "password for {input:?}");
            assert_eq!(output, feedback, "feedback for {input:?}");
        }
    }

    /// `^D` at an empty password is the typed EOF (no silent
    /// empty-string answer).
    #[test]
    fn ctrl_d_at_an_empty_password_is_typed_eof() {
        let (outcome, _feedback) = grammar_row(b"\x04");
        assert_eq!(
            outcome.expect("typed kind"),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    /// The `^C` arm: typed `Interrupted` and the abort newline, with
    /// the SIGINT raise observed for real through a safe handler (the
    /// raise parity is Q11's duality; its end-to-end effect stays
    /// pinned by the pty tier's `prompt ∥ cancelled` leg).
    #[test]
    fn ctrl_c_is_typed_interrupted_and_raises_sigint() {
        let raised = Arc::new(AtomicBool::new(false));
        let _registration =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&raised))
                .expect("register the SIGINT observer");
        let (outcome, feedback) = grammar_row(b"\x03");
        assert_eq!(
            outcome.expect("typed kind"),
            std::io::ErrorKind::Interrupted
        );
        assert_eq!(feedback, b"\n");
        assert!(raised.load(Ordering::SeqCst), "the raise parity must hold");
    }

    /// THE WEDGE PIN (#572's cure, exercised deterministically): the
    /// read armed with no input, the wake fired once the read is armed
    /// — the core returns the typed interruption, consumed nothing,
    /// and the termios are restored (every exit path, the
    /// dropped-restore parity).
    #[test]
    fn a_wake_unblocks_an_armed_read_with_typed_interruption() {
        let (_master, slave) = pty();
        let (sink, source) = wakeup_channel().expect("wake channel");
        let (result, feedback) = run_with(&slave, Some(&source), || sink.fire());
        assert!(matches!(
            result,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted
        ));
        assert!(feedback.is_empty(), "no input was consumed: {feedback:?}");
        let restored = tcgetattr(slave.as_fd()).expect("probe the termios");
        assert!(restored.local_flags.contains(
            nix::sys::termios::LocalFlags::ISIG
                | nix::sys::termios::LocalFlags::ICANON
                | nix::sys::termios::LocalFlags::ECHO
        ));
    }

    /// The stale-wake drain (D5): a wake consumed by one read must
    /// never abandon the next one.
    #[test]
    fn a_drained_wake_never_abandons_the_next_read() {
        let (master, slave) = pty();
        let (sink, source) = wakeup_channel().expect("wake channel");
        let (result, _feedback) = run_with(&slave, Some(&source), || sink.fire());
        assert!(matches!(
            result,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted
        ));
        // A fresh prompt with no new wake completes: the stale wake
        // byte did not survive the first read.
        let (result, feedback) = run_with(&slave, Some(&source), || {
            feed(&master, b"hi\n");
        });
        assert_eq!(result.expect("the second read completes"), "hi");
        assert_eq!(feedback, b"**\n");
    }

    /// A wake ready alongside a readable byte processes the byte, then
    /// takes the wake — no lost wake, no corrupted state.
    #[test]
    fn a_byte_then_a_wake_takes_the_byte_and_then_the_cancellation() {
        let (master, slave) = pty();
        let (sink, source) = wakeup_channel().expect("wake channel");
        let (result, feedback) = run_with(&slave, Some(&source), || {
            feed(&master, b"a");
            sink.fire();
        });
        assert!(matches!(
            result,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted
        ));
        assert_eq!(feedback, b"*", "the byte was consumed before the wake");
    }

    /// A wake mid-character still delivers the typed interruption.
    #[test]
    fn a_wake_mid_character_interrupts() {
        let (master, slave) = pty();
        let (sink, source) = wakeup_channel().expect("wake channel");
        let (result, _feedback) = run_with(&slave, Some(&source), || {
            feed(&master, &[0xC3]);
            sink.fire();
        });
        assert!(matches!(
            result,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted
        ));
    }

    /// Feedback-state parity spot checks against vendored
    /// `feedback.rs` (the units under the grammar table): every
    /// displayed mask is erased by its own pop.
    #[test]
    fn the_mask_state_matches_the_vendored_feedback() {
        let mut state = MaskState::default();
        assert_eq!(state.push('a'), "*");
        assert_eq!(state.push('🚲'), "*");
        assert_eq!(state.pop(), "\x08 \x08");
        assert_eq!(state.pop(), "\x08 \x08");
        assert_eq!(state.pop(), "");
        state.push('x');
        state.push('y');
        state.push('z');
        assert_eq!(state.clear(), "\x08 \x08\x08 \x08\x08 \x08");
        assert_eq!(state.password, "");
        state.push('a');
        state.push(' ');
        state.push('b');
        assert_eq!(state.clear_til_last_space(), "\x08 \x08");
        assert_eq!(state.password, "a ");
        assert_eq!(state.abort(), "\n");
        assert_eq!(state.password, "");
    }

    /// The wake sink is cloneable (the session hands a clone to
    /// `ReplPrompts` while `EditorIo` keeps one) and a clone fires the
    /// same source.
    #[test]
    fn a_cloned_sink_fires_the_same_source() {
        let (sink, source) = wakeup_channel().expect("wake channel");
        sink.clone().fire();
        source.drain();
        // Drained: a fresh fire delivers exactly one readable byte.
        sink.fire();
        let mut buf = [0u8; 8];
        let n = nix::unistd::read(source.as_fd(), &mut buf).expect("read the wake");
        assert_eq!(n, 1);
    }
}
