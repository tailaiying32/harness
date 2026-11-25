//! Keep awake: holds the computer (not the display) awake while the user is
//! away, so chats keep working. The one exception is a Windows laptop with
//! Modern Standby on battery, where only a lit screen holds sleep off (see
//! `platform::modern_standby`). One for the whole app; the sidebar button and
//! an agent tool both start it. It ends on its own at the first of:
//!
//! - the user is back: once there has been no keyboard or mouse input for
//!   `AWAY` (the user left; the screen is dark or locked by then), the next
//!   input ends it. Input before that, like the click that started it, doesn't.
//! - the work is done: no chat has worked for `QUIET`, once one has worked
//!   since it started. A chat waiting for the user doesn't count as working
//!   (staying awake can't move it on). Background work does: commands like a
//!   test run, and subagents. A command that never ends (a dev server) holds
//!   it until its end time, which is the safety net.
//! - its end time: `LIMIT` after starting, or a clock time for overnight,
//!   which also needs the charger and ends when it is unplugged.
//!
//! The hold is released when it ends, when the app quits, and on Mac even if
//! the app crashes (`caffeinate -w`), so none is ever left behind.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::events::{AgentEvent, Envelope, SessionState};
use crate::sessions::Out;
use crate::store::now_ms;

/// The name the agent knows the tool group by.
pub const SERVER: &str = "tilde_keep_awake";

/// The line added to the agent's instructions.
pub const INSTRUCTION: &str = "When the user asks to keep the computer awake, caffeinate, or keep the \
session alive while they're away, use the tilde_keep_awake tools instead of running caffeinate.";

/// How long a plain keep-awake lasts at most.
const LIMIT: Duration = Duration::from_secs(2 * 60 * 60);
/// No input for this long means the user has left.
const AWAY: Duration = Duration::from_secs(2 * 60);
/// No chat working for this long means the work is done.
const QUIET: Duration = Duration::from_secs(60);
/// How often the rules are checked.
const TICK: Duration = Duration::from_secs(15);
/// The furthest an overnight end time may be.
const MAX_OVERNIGHT: Duration = Duration::from_secs(16 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Stepping away: ends at `LIMIT` at most.
    Away,
    /// Ends at a clock time, plugged in only.
    Overnight,
}

/// What the window shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeepAwakeState {
    pub mode: Mode,
    /// When it ends at the latest, in ms since 1970.
    pub ends_at: u64,
}

#[derive(Clone, Copy)]
struct Config {
    limit: Duration,
    away: Duration,
    quiet: Duration,
    tick: Duration,
}

impl Config {
    /// The defaults, with `TILDE_KEEP_AWAKE_*_SECONDS` overrides for tests.
    fn from_env() -> Self {
        let secs = |name: &str, default: Duration| {
            crate::home::app_env_text(&format!("KEEP_AWAKE_{name}_SECONDS"))
                .and_then(|v| v.parse().ok())
                .map(Duration::from_secs)
                .unwrap_or(default)
        };
        Self {
            limit: secs("LIMIT", LIMIT),
            away: secs("AWAY", AWAY),
            quiet: secs("QUIET", QUIET),
            tick: secs("TICK", TICK),
        }
    }
}

struct Active {
    mode: Mode,
    ends_at: u64,
    /// The user has been away since it started.
    armed: bool,
    /// Some chat has worked since it started.
    seen_work: bool,
    /// When the last chat stopped working, after one had.
    quiet_since: Option<Instant>,
    /// Released when dropped.
    _hold: Hold,
}

/// One check's inputs.
struct Reading {
    now_ms: u64,
    now: Instant,
    idle: Duration,
    on_battery: bool,
    any_working: bool,
}

/// Applies the rules once. True means it should end.
fn decide(a: &mut Active, r: &Reading, cfg: &Config) -> bool {
    if r.now_ms >= a.ends_at {
        return true;
    }
    if a.mode == Mode::Overnight && r.on_battery {
        return true;
    }
    if !a.armed {
        if r.idle >= cfg.away {
            a.armed = true;
        }
    } else if r.idle < cfg.away {
        return true;
    }
    if r.any_working {
        a.seen_work = true;
        a.quiet_since = None;
    } else if a.seen_work {
        let since = *a.quiet_since.get_or_insert(r.now);
        if r.now.duration_since(since) >= cfg.quiet {
            return true;
        }
    }
    false
}

/// What each chat is doing: its state and how much background work it runs.
#[derive(Default)]
struct Chats(HashMap<String, (SessionState, u32)>);

impl Chats {
    /// Takes in one event. Side chats are left out.
    fn observe(&mut self, envelope: &Envelope) {
        if envelope.session_id.is_empty() || envelope.session_id.contains("#side-") {
            return;
        }
        let id = &envelope.session_id;
        match &envelope.event {
            AgentEvent::Status { state: SessionState::Exited } => {
                self.0.remove(id);
            }
            AgentEvent::Status { state } => {
                self.0.entry(id.clone()).or_insert((SessionState::Idle, 0)).0 = state.clone();
            }
            AgentEvent::BackgroundWork { running, .. } => {
                self.0.entry(id.clone()).or_insert((SessionState::Idle, 0)).1 = *running;
            }
            _ => {}
        }
    }

    fn any_working(&self) -> bool {
        self.0
            .values()
            .any(|(state, running)| matches!(state, SessionState::Starting | SessionState::Working) || *running > 0)
    }
}

struct Inner {
    active: Mutex<Option<Active>>,
    chats: Mutex<Chats>,
    /// Held while any chat works, so the system doesn't slow the app down
    /// (App Nap on Mac) while its window is out of sight.
    no_nap: Mutex<Option<NoNap>>,
    /// To the window. Not the wrapped one that feeds `observe`.
    out: Out,
    cfg: Config,
}

#[derive(Clone)]
pub struct KeepAwake(Arc<Inner>);

impl KeepAwake {
    pub fn new(out: Out) -> Self {
        Self(Arc::new(Inner {
            active: Mutex::default(),
            chats: Mutex::default(),
            no_nap: Mutex::default(),
            out,
            cfg: Config::from_env(),
        }))
    }

    /// Takes in every chat event, to know which chats are working.
    pub fn observe(&self, envelope: &Envelope) {
        if !matches!(envelope.event, AgentEvent::Status { .. } | AgentEvent::BackgroundWork { .. }) {
            return;
        }
        let working = {
            let mut chats = self.0.chats.lock().unwrap();
            chats.observe(envelope);
            chats.any_working()
        };
        {
            let mut no_nap = self.0.no_nap.lock().unwrap();
            if working && no_nap.is_none() {
                *no_nap = Some(NoNap::take());
            } else if !working {
                no_nap.take();
            }
        }
        if working && let Some(a) = self.0.active.lock().unwrap().as_mut() {
            a.seen_work = true;
            a.quiet_since = None;
        }
    }

    pub fn state(&self) -> Option<KeepAwakeState> {
        self.0
            .active
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| KeepAwakeState { mode: a.mode, ends_at: a.ends_at })
    }

    /// Starts it, or changes the mode and end time of the one running.
    /// Overnight takes its end time; away ends `LIMIT` from now.
    pub fn start(&self, mode: Mode, ends_at: Option<u64>) -> Result<KeepAwakeState, String> {
        let now = now_ms().max(0) as u64;
        let ends_at = match mode {
            Mode::Away => now + self.0.cfg.limit.as_millis() as u64,
            Mode::Overnight => {
                let ends_at = ends_at.ok_or("Give an end time for overnight.")?;
                if ends_at <= now {
                    return Err("That end time has passed.".into());
                }
                if ends_at - now > MAX_OVERNIGHT.as_millis() as u64 {
                    return Err("That end time is more than 16 hours away.".into());
                }
                if on_battery() {
                    return Err("The computer is on battery. Overnight needs the charger: plug in first.".into());
                }
                ends_at
            }
        };
        {
            let mut active = self.0.active.lock().unwrap();
            match active.as_mut() {
                Some(a) => {
                    a.mode = mode;
                    a.ends_at = ends_at;
                }
                None => {
                    *active = Some(Active {
                        mode,
                        ends_at,
                        armed: false,
                        seen_work: self.0.chats.lock().unwrap().any_working(),
                        quiet_since: None,
                        _hold: Hold::take()?,
                    })
                }
            }
        }
        self.send();
        Ok(KeepAwakeState { mode, ends_at })
    }

    pub fn stop(&self) {
        if self.0.active.lock().unwrap().take().is_some() {
            self.send();
        }
    }

    /// Releases the hold when the app quits.
    pub fn shutdown(&self) {
        self.0.active.lock().unwrap().take();
    }

    /// Checks the rules every tick, for as long as the app runs.
    pub async fn run(self) {
        loop {
            tokio::time::sleep(self.0.cfg.tick).await;
            let Some(mode) = self.0.active.lock().unwrap().as_ref().map(|a| a.mode) else {
                continue;
            };
            // Asking about the battery starts a process on Mac: only overnight needs it.
            let on_battery = mode == Mode::Overnight
                && tauri::async_runtime::spawn_blocking(on_battery).await.unwrap_or(false);
            let reading = Reading {
                now_ms: now_ms().max(0) as u64,
                now: Instant::now(),
                idle: idle(),
                on_battery,
                any_working: self.0.chats.lock().unwrap().any_working(),
            };
            let ended = {
                let mut active = self.0.active.lock().unwrap();
                let end = active.as_mut().is_some_and(|a| decide(a, &reading, &self.0.cfg));
                end && active.take().is_some()
            };
            if ended {
                self.send();
            }
        }
    }

    fn send(&self) {
        let state = self.state();
        (self.0.out)(Envelope {
            session_id: String::new(),
            event: AgentEvent::KeepAwake {
                mode: state.as_ref().map(|s| s.mode),
                ends_at: state.map(|s| s.ends_at),
            },
        });
    }

    /// Runs one of the group's tools.
    pub async fn call(&self, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = match tool {
            "keep_awake" => {
                let state = match args["until"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(until) => {
                        let time = parse_clock(until).ok_or("Give `until` as HH:MM, like 07:30.")?;
                        let ends_at = next_at(Local::now(), time).timestamp_millis() as u64;
                        let this = self.clone();
                        tauri::async_runtime::spawn_blocking(move || this.start(Mode::Overnight, Some(ends_at)))
                            .await
                            .map_err(|e| e.to_string())??
                    }
                    None => self.start(Mode::Away, None)?,
                };
                let at = clock(state.ends_at);
                match state.mode {
                    Mode::Away => format!(
                        "Keeping the computer awake until {at} at most. It ends sooner when the user is back \
or when no chat is working."
                    ),
                    Mode::Overnight => format!(
                        "Keeping the computer awake until {at} (overnight, while plugged in). It ends sooner when \
the user is back or when no chat is working."
                    ),
                }
            }
            "stop_keep_awake" => {
                self.stop();
                "Stopped keeping the computer awake.".into()
            }
            other => return Err(format!("No tool named {other}")),
        };
        Ok(vec![json!({ "type": "text", "text": text })])
    }
}

/// The group's tool definitions.
pub fn tools() -> Value {
    json!([
        {
            "name": "keep_awake",
            "description": "Keep the computer awake (the screen still turns off, except on a Windows laptop on battery) so chats keep working while \
the user is away. Use it instead of running caffeinate when the user asks to keep the computer awake, \
caffeinate, or keep the session alive. It ends on its own when the user is back, when no chat is working, \
or after 2 hours. Give `until` only when the user asks for a later time, such as overnight; that needs the \
computer plugged in.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "until": { "type": "string", "description": "A local clock time, HH:MM (24-hour), like 07:30. Only when the user asks for a later end, such as overnight." },
                },
            },
        },
        {
            "name": "stop_keep_awake",
            "description": "Stop keeping the computer awake, when the user asks.",
            "inputSchema": { "type": "object", "properties": {} },
        },
    ])
}

/// "7:30" or "07:30" as a time of day.
fn parse_clock(text: &str) -> Option<NaiveTime> {
    let (h, m) = text.split_once(':')?;
    NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
}

/// The next moment after `now` that the clock shows `time`: today if still
/// ahead, else tomorrow. A time skipped by a clock change moves an hour later.
fn next_at<Tz: TimeZone>(now: DateTime<Tz>, time: NaiveTime) -> DateTime<Tz> {
    let tz = now.timezone();
    let mut day = now.date_naive();
    loop {
        let mut at = day.and_time(time);
        let found = loop {
            if let Some(dt) = tz.from_local_datetime(&at).earliest() {
                break dt;
            }
            at += chrono::Duration::hours(1);
        };
        if found > now {
            return found;
        }
        day = day.succ_opt().expect("a next day");
    }
}

/// "08:00", in local time.
fn clock(ms: u64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub use platform::on_battery;
use platform::{Hold, NoNap, idle};

#[cfg(target_os = "macos")]
mod platform {
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    /// A `caffeinate` that holds idle sleep off (not display sleep) and exits
    /// with the app, even if the app crashes.
    pub struct Hold(pub(super) Child);

    impl Hold {
        pub fn take() -> Result<Self, String> {
            Command::new("/usr/bin/caffeinate")
                .args(["-i", "-w", &std::process::id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(Hold)
                .map_err(|e| format!("Can't keep the computer awake: {e}"))
        }
    }

    impl Drop for Hold {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Keeps App Nap off: a covered or hidden app is otherwise slowed, and
    /// with it the agents' tool calls and the browser's pages.
    pub struct NoNap(objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>);

    // SAFETY: the token is an opaque object NSProcessInfo hands back; ending it
    // from any thread is allowed.
    unsafe impl Send for NoNap {}

    impl NoNap {
        pub fn take() -> Self {
            use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
            let info = NSProcessInfo::processInfo();
            let reason = NSString::from_str("A chat is working");
            NoNap(info.beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep, &reason))
        }
    }

    impl Drop for NoNap {
        fn drop(&mut self) {
            // SAFETY: the token from `beginActivityWithOptions`, ended once.
            unsafe { objc2_foundation::NSProcessInfo::processInfo().endActivity(&self.0) };
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(source: i32, event: u32) -> f64;
    }

    /// Time since the last keyboard or mouse input.
    pub fn idle() -> Duration {
        const HID_SYSTEM_STATE: i32 = 1;
        const ANY_INPUT: u32 = !0;
        // SAFETY: a plain C call with two integers; no pointers involved.
        let secs = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT) };
        Duration::from_secs_f64(secs.max(0.0))
    }

    /// Whether the Mac runs on battery. Unknown counts as plugged in.
    pub fn on_battery() -> bool {
        Command::new("/usr/bin/pmset")
            .args(["-g", "ps"])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .is_some_and(|text| text.lines().next().is_some_and(|l| l.contains("'Battery Power'")))
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, GetPwrCapabilities, GetSystemPowerStatus,
        SYSTEM_POWER_CAPABILITIES, SYSTEM_POWER_STATUS, SetThreadExecutionState,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    /// Windows has no App Nap to hold off.
    pub struct NoNap;

    impl NoNap {
        pub fn take() -> Self {
            NoNap
        }
    }

    /// How often the hold looks again at whether the screen must stay on.
    const RECHECK: Duration = Duration::from_secs(30);

    /// A thread that holds the system awake: Windows ties the request to the
    /// thread that made it, so the same thread lets it go.
    pub struct Hold(pub(super) Option<mpsc::Sender<()>>);

    impl Hold {
        pub fn take() -> Result<Self, String> {
            let (tx, rx) = mpsc::channel::<()>();
            let modern = modern_standby();
            std::thread::Builder::new()
                .name("keep-awake".into())
                .spawn(move || loop {
                    let flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED | if modern && on_battery() { ES_DISPLAY_REQUIRED } else { 0 };
                    // SAFETY: plain calls with flag values.
                    unsafe { SetThreadExecutionState(flags) };
                    if !matches!(rx.recv_timeout(RECHECK), Err(mpsc::RecvTimeoutError::Timeout)) {
                        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                        return;
                    }
                })
                .map_err(|e| format!("Can't keep the computer awake: {e}"))?;
            Ok(Hold(Some(tx)))
        }
    }

    /// Whether the PC sleeps the Modern Standby way, where sleep starts when
    /// the screen goes off. There, on battery, Windows drops a system-awake
    /// request 5 minutes after the sleep timeout, and only a screen kept on
    /// holds sleep off; so on battery the screen stays on too. Plugged in, the
    /// request holds and the screen may go dark as usual.
    fn modern_standby() -> bool {
        // SAFETY: an all-zero SYSTEM_POWER_CAPABILITIES is valid; the call fills it.
        let mut caps: SYSTEM_POWER_CAPABILITIES = unsafe { std::mem::zeroed() };
        unsafe { GetPwrCapabilities(&mut caps) && caps.AoAc }
    }

    impl Drop for Hold {
        fn drop(&mut self) {
            // Dropping the sender ends the thread's wait.
            self.0.take();
        }
    }

    /// Time since the last keyboard or mouse input.
    pub fn idle() -> Duration {
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        // SAFETY: `info` is a valid struct with its size set.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return Duration::ZERO;
        }
        let now = unsafe { GetTickCount() };
        Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
    }

    /// Whether the computer runs on battery. Unknown counts as plugged in.
    pub fn on_battery() -> bool {
        // SAFETY: an all-zero SYSTEM_POWER_STATUS is valid; the call fills it.
        let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        unsafe { GetSystemPowerStatus(&mut status) != 0 && status.ACLineStatus == 0 }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use std::time::Duration;

    pub struct Hold;

    pub struct NoNap;

    impl NoNap {
        pub fn take() -> Self {
            NoNap
        }
    }

    impl Hold {
        pub fn take() -> Result<Self, String> {
            Ok(Hold)
        }
    }

    pub fn idle() -> Duration {
        Duration::ZERO
    }

    pub fn on_battery() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BackgroundTask;
    use chrono::{Datelike, FixedOffset, Timelike};

    const CFG: Config = Config {
        limit: Duration::from_secs(7200),
        away: Duration::from_secs(120),
        quiet: Duration::from_secs(60),
        tick: Duration::from_secs(15),
    };

    fn active(mode: Mode) -> Active {
        Active {
            mode,
            ends_at: 10_000_000,
            armed: false,
            seen_work: false,
            quiet_since: None,
            _hold: test_hold(),
        }
    }

    #[cfg(target_os = "macos")]
    fn test_hold() -> Hold {
        // A real `caffeinate` would hold the test machine awake; `true` exits at once.
        Hold(std::process::Command::new("true").spawn().unwrap())
    }
    #[cfg(windows)]
    fn test_hold() -> Hold {
        Hold(None)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    fn test_hold() -> Hold {
        Hold
    }

    fn reading(now: Instant, idle_secs: u64, working: bool) -> Reading {
        Reading {
            now_ms: 1_000,
            now,
            idle: Duration::from_secs(idle_secs),
            on_battery: false,
            any_working: working,
        }
    }

    #[test]
    fn ends_at_its_end_time() {
        let mut a = active(Mode::Away);
        let mut r = reading(Instant::now(), 0, true);
        r.now_ms = a.ends_at;
        assert!(decide(&mut a, &r, &CFG));
    }

    #[test]
    fn overnight_ends_on_battery_and_away_does_not() {
        let now = Instant::now();
        let mut r = reading(now, 0, true);
        r.on_battery = true;
        assert!(decide(&mut active(Mode::Overnight), &r, &CFG));
        assert!(!decide(&mut active(Mode::Away), &r, &CFG));
    }

    #[test]
    fn input_ends_it_only_after_the_user_left() {
        let now = Instant::now();
        let mut a = active(Mode::Away);
        // Using the computer right after starting it doesn't end it.
        for idle in [0, 5, 119, 0] {
            assert!(!decide(&mut a, &reading(now, idle, true), &CFG));
        }
        assert!(!a.armed);
        // Two minutes without input: the user left.
        assert!(!decide(&mut a, &reading(now, 120, true), &CFG));
        assert!(a.armed);
        assert!(!decide(&mut a, &reading(now, 900, true), &CFG));
        // Input again: back.
        assert!(decide(&mut a, &reading(now, 3, true), &CFG));
    }

    #[test]
    fn ends_once_work_has_stopped_for_a_while() {
        let start = Instant::now();
        let mut a = active(Mode::Away);
        assert!(!decide(&mut a, &reading(start, 0, true), &CFG));
        assert!(!decide(&mut a, &reading(start, 0, false), &CFG));
        assert!(!decide(&mut a, &reading(start + Duration::from_secs(59), 0, false), &CFG));
        // Work again resets the wait.
        assert!(!decide(&mut a, &reading(start + Duration::from_secs(70), 0, true), &CFG));
        assert!(!decide(&mut a, &reading(start + Duration::from_secs(80), 0, false), &CFG));
        assert!(!decide(&mut a, &reading(start + Duration::from_secs(139), 0, false), &CFG));
        assert!(decide(&mut a, &reading(start + Duration::from_secs(140), 0, false), &CFG));
    }

    #[test]
    fn no_work_yet_never_counts_as_done() {
        let start = Instant::now();
        let mut a = active(Mode::Away);
        for secs in [0, 60, 600, 3600] {
            assert!(!decide(&mut a, &reading(start + Duration::from_secs(secs), 0, false), &CFG));
        }
    }

    fn envelope(id: &str, event: AgentEvent) -> Envelope {
        Envelope { session_id: id.into(), event }
    }

    fn background(running: u32, agents: u32) -> AgentEvent {
        AgentEvent::BackgroundWork { running, agents, tasks: Vec::<BackgroundTask>::new() }
    }

    #[test]
    fn what_counts_as_working() {
        let mut chats = Chats::default();
        let status = |state| AgentEvent::Status { state };
        chats.observe(&envelope("a", status(SessionState::Working)));
        assert!(chats.any_working());
        // Waiting for the user isn't working.
        chats.observe(&envelope("a", status(SessionState::WaitingApproval)));
        assert!(!chats.any_working());
        // Background work is: commands and subagents alike.
        chats.observe(&envelope("a", status(SessionState::Idle)));
        assert!(!chats.any_working());
        chats.observe(&envelope("a", background(2, 0)));
        assert!(chats.any_working());
        chats.observe(&envelope("a", background(3, 1)));
        assert!(chats.any_working());
        chats.observe(&envelope("a", background(0, 0)));
        assert!(!chats.any_working());
        // A process that exits is done, background work and all.
        chats.observe(&envelope("a", background(1, 0)));
        chats.observe(&envelope("a", status(SessionState::Exited)));
        assert!(!chats.any_working());
        // Side chats don't count.
        chats.observe(&envelope("a#side-1", status(SessionState::Working)));
        assert!(!chats.any_working());
        chats.observe(&envelope("b", status(SessionState::Starting)));
        assert!(chats.any_working());
    }

    #[test]
    fn next_clock_time_is_today_or_tomorrow() {
        let tz = FixedOffset::east_opt(-8 * 3600).unwrap();
        let eight = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        let evening = tz.with_ymd_and_hms(2026, 11, 17, 22, 30, 0).unwrap();
        let at = next_at(evening, eight);
        assert_eq!((at.day(), at.hour(), at.minute()), (18, 8, 0));
        let early = tz.with_ymd_and_hms(2026, 11, 17, 6, 0, 0).unwrap();
        assert_eq!(next_at(early, eight).day(), 17);
        // Exactly now means tomorrow.
        let on_time = tz.with_ymd_and_hms(2026, 11, 17, 8, 0, 0).unwrap();
        assert_eq!(next_at(on_time, eight).day(), 18);
    }

    #[test]
    fn reads_clock_times() {
        assert_eq!(parse_clock("7:30"), NaiveTime::from_hms_opt(7, 30, 0));
        assert_eq!(parse_clock("23:05"), NaiveTime::from_hms_opt(23, 5, 0));
        assert_eq!(parse_clock("25:00"), None);
        assert_eq!(parse_clock("8am"), None);
    }

}
