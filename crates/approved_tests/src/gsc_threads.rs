//! GSC thread semantics (`gsc_threads`): what a ported script can rely on when
//! it waits, notifies, ends on an event or waits on a flag. The world is a log
//! of what the threads did, so each scenario reads as the order of events.

use gsc_threads::{Cx, Owner, Scheduler, Thread, Value, Yield};

type Log = Vec<String>;

const FRAME_MS: u64 = 50;

fn run_frames(sched: &mut Scheduler<Log>, log: &mut Log, frames: u64) {
    for frame in 0..frames {
        sched.run(frame * FRAME_MS, log);
    }
}

/// `wait(seconds); log(tag)` then return.
#[derive(Clone, Debug)]
struct WaitThenLog {
    ms: u64,
    tag: &'static str,
    waited: bool,
}

impl Thread<Log> for WaitThenLog {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if !self.waited {
            self.waited = true;
            return Yield::Wait { ms: self.ms };
        }
        let line = format!("{} at {}", self.tag, cx.now_ms());
        cx.world.push(line);
        Yield::Done
    }
}

/// `self endon(endon); owner waittill(name, amount); log(amount)`.
#[derive(Clone, Debug)]
struct WaitTillLog {
    owner: Owner,
    name: &'static str,
    endon: Option<&'static str>,
    started: bool,
}

impl Thread<Log> for WaitTillLog {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if !self.started {
            self.started = true;
            if let Some(endon) = self.endon {
                cx.endon(cx.owner(), endon);
            }
            return Yield::waittill(self.owner, self.name);
        }
        let amount = cx.event().and_then(|event| event.args.first().cloned());
        cx.world.push(format!("{} woke with {amount:?}", self.name));
        Yield::Done
    }
}

/// `level notify(name, 42)` then return.
#[derive(Clone, Debug)]
struct Notifier {
    owner: Owner,
    name: &'static str,
}

impl Thread<Log> for Notifier {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        cx.world.push(format!("notify {}", self.name));
        cx.notify(self.owner, self.name, vec![Value::Int(42)]);
        Yield::Done
    }
}

/// `flag_wait(name); log`.
#[derive(Clone, Debug)]
struct FlagWaiter {
    flag: &'static str,
    waited: bool,
}

impl Thread<Log> for FlagWaiter {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if !self.waited {
            self.waited = true;
            return Yield::FlagWait(self.flag.into());
        }
        let line = format!("flag {} seen at {}", self.flag, cx.now_ms());
        cx.world.push(line);
        Yield::Done
    }
}

/// `wait(ms); flag_set(name)`.
#[derive(Clone, Debug)]
struct FlagSetter {
    flag: &'static str,
    ms: u64,
    waited: bool,
}

impl Thread<Log> for FlagSetter {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if !self.waited {
            self.waited = true;
            return Yield::Wait { ms: self.ms };
        }
        cx.flag_set(self.flag);
        Yield::Done
    }
}

/// `log("parent"); thread child(); log("parent after")`.
#[derive(Clone, Debug)]
struct Parent;

impl Thread<Log> for Parent {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        cx.world.push("parent".into());
        cx.thread(
            Owner::LEVEL,
            WaitThenLog {
                ms: 0,
                tag: "child",
                waited: true,
            },
        );
        cx.world.push("parent after".into());
        Yield::Done
    }
}

/// `for (;;) { flag_wait(name); }` on a set flag: never yields a frame.
#[derive(Clone, Debug)]
struct Spinner;

impl Thread<Log> for Spinner {
    fn resume(&mut self, _cx: &mut Cx<'_, Log>) -> Yield {
        Yield::FlagWait("always".into())
    }
}

/// A `wait 0.05` loop that counts its frames.
#[derive(Clone, Debug)]
struct Ticker {
    n: u32,
}

impl Thread<Log> for Ticker {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        self.n += 1;
        cx.world.push(format!("tick {} at {}", self.n, cx.now_ms()));
        if self.n == 3 {
            Yield::Done
        } else {
            Yield::wait_seconds(0.05)
        }
    }
}

#[test]
fn wait_resumes_on_the_first_frame_at_or_after_its_time() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(
        Owner::LEVEL,
        WaitThenLog {
            ms: 120,
            tag: "a",
            waited: false,
        },
    );
    run_frames(&mut sched, &mut log, 6);
    assert_eq!(log, ["a at 150"]);
    assert_eq!(sched.thread_count(), 0);
}

#[test]
fn wait_005_is_one_server_frame() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(Owner::LEVEL, Ticker { n: 0 });
    run_frames(&mut sched, &mut log, 5);
    assert_eq!(log, ["tick 1 at 0", "tick 2 at 50", "tick 3 at 100"]);
}

#[test]
fn notify_wakes_the_waiter_the_same_frame_with_its_arguments() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(
        Owner::LEVEL,
        WaitTillLog {
            owner: Owner::LEVEL,
            name: "start_round",
            endon: None,
            started: false,
        },
    );
    sched.spawn(
        Owner::LEVEL,
        Notifier {
            owner: Owner::LEVEL,
            name: "start_round",
        },
    );
    run_frames(&mut sched, &mut log, 1);
    assert_eq!(
        log,
        ["notify start_round", "start_round woke with Some(Int(42))"]
    );
}

#[test]
fn a_notify_on_another_owner_does_not_wake() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(
        Owner(7),
        WaitTillLog {
            owner: Owner(7),
            name: "damage",
            endon: None,
            started: false,
        },
    );
    run_frames(&mut sched, &mut log, 1);
    sched.notify(Owner(8), "damage", Vec::new());
    run_frames(&mut sched, &mut log, 2);
    assert!(log.is_empty());
    assert_eq!(sched.thread_count(), 1);
}

#[test]
fn endon_kills_the_thread_before_it_can_resume() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    let zombie = Owner(3);
    sched.spawn(
        zombie,
        WaitTillLog {
            owner: Owner::LEVEL,
            name: "end_of_round",
            endon: Some("death"),
            started: false,
        },
    );
    run_frames(&mut sched, &mut log, 1);
    assert_eq!(sched.notify(zombie, "death", Vec::new()), 1);
    sched.notify(Owner::LEVEL, "end_of_round", Vec::new());
    run_frames(&mut sched, &mut log, 2);
    assert!(log.is_empty());
    assert_eq!(sched.thread_count(), 0);
}

#[test]
fn flag_set_wakes_flag_waiters_and_a_set_flag_does_not_block() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.flag_init("power_on");
    sched.spawn(
        Owner::LEVEL,
        FlagWaiter {
            flag: "power_on",
            waited: false,
        },
    );
    sched.spawn(
        Owner::LEVEL,
        FlagSetter {
            flag: "power_on",
            ms: 100,
            waited: false,
        },
    );
    run_frames(&mut sched, &mut log, 4);
    sched.spawn(
        Owner::LEVEL,
        FlagWaiter {
            flag: "power_on",
            waited: false,
        },
    );
    sched.run(200, &mut log);
    assert_eq!(
        log,
        ["flag power_on seen at 100", "flag power_on seen at 200"]
    );
}

#[test]
fn a_spawned_thread_runs_later_in_the_same_frame() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(Owner::LEVEL, Parent);
    run_frames(&mut sched, &mut log, 1);
    assert_eq!(log, ["parent", "parent after", "child at 0"]);
}

#[test]
fn a_thread_that_never_waits_is_stopped_and_reported() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.flag_init("always");
    sched.spawn(
        Owner::LEVEL,
        FlagSetter {
            flag: "always",
            ms: 0,
            waited: true,
        },
    );
    sched.spawn(Owner::LEVEL, Spinner);
    let report = sched.run(0, &mut log);
    assert!(report.runaway);
    assert_eq!(report.resumed, Scheduler::<Log>::MAX_RESUMES_PER_FRAME);
}

#[test]
fn a_cloned_scheduler_continues_exactly_like_the_original() {
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(Owner::LEVEL, Ticker { n: 0 });
    sched.spawn(
        Owner::LEVEL,
        WaitThenLog {
            ms: 75,
            tag: "late",
            waited: false,
        },
    );
    sched.run(0, &mut log);
    let mut copy = sched.clone();
    let mut copy_log = log.clone();
    for frame in 1..5 {
        sched.run(frame * FRAME_MS, &mut log);
        copy.run(frame * FRAME_MS, &mut copy_log);
    }
    assert_eq!(log, copy_log);
    assert_eq!(
        log,
        [
            "tick 1 at 0",
            "tick 2 at 50",
            "tick 3 at 100",
            "late at 100"
        ]
    );
}

/// `for (;;) { self waittill(name, note); log(note); if (note == "end") break; }`.
#[derive(Clone, Debug)]
struct NoteLoop {
    name: &'static str,
    end: &'static str,
    started: bool,
}

impl Thread<Log> for NoteLoop {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if std::mem::replace(&mut self.started, true) {
            let note = cx.event().and_then(|event| event.args.first().cloned());
            cx.world.push(format!("{note:?}"));
            if note == Some(Value::Str(self.end.into())) {
                return Yield::Done;
            }
        }
        Yield::waittill(cx.owner(), self.name)
    }
}

#[test]
fn every_notify_of_one_frame_reaches_the_waiter_in_order() {
    const NOTE: &str = "meleeanim";
    const END: &str = "end";
    let owner = Owner(7);
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(
        owner,
        NoteLoop {
            name: NOTE,
            end: END,
            started: false,
        },
    );
    run_frames(&mut sched, &mut log, 1);
    for note in ["fire", "fire", END] {
        sched.notify(owner, NOTE, vec![Value::Str(note.into())]);
    }
    sched.run(FRAME_MS, &mut log);
    assert_eq!(
        log,
        [
            r#"Some(Str("fire"))"#,
            r#"Some(Str("fire"))"#,
            r#"Some(Str("end"))"#
        ]
    );
    assert_eq!(sched.thread_count(), 0, "the end note ends the loop");
}

/// `self waittill("go"); self notify("ping"); self waittill("ping"); log`.
#[derive(Clone, Debug, Default)]
struct PingSelf {
    pc: u8,
}

impl Thread<Log> for PingSelf {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        let owner = cx.owner();
        self.pc += 1;
        match self.pc {
            1 => Yield::waittill(owner, "go"),
            2 => {
                cx.notify(owner, "ping", Vec::new());
                Yield::waittill(owner, "ping")
            }
            _ => {
                cx.world.push(format!("pinged at {}", cx.now_ms()));
                Yield::Done
            }
        }
    }
}

#[test]
fn a_thread_does_not_hear_its_own_notify() {
    let owner = Owner(3);
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(owner, PingSelf::default());
    run_frames(&mut sched, &mut log, 1);
    sched.notify(owner, "go", Vec::new());
    sched.run(FRAME_MS, &mut log);
    assert!(log.is_empty(), "{log:?}");
    sched.notify(owner, "ping", Vec::new());
    sched.run(2 * FRAME_MS, &mut log);
    assert_eq!(log, [format!("pinged at {}", 2 * FRAME_MS)]);
}

/// `for (;;) { self waittill("note", n); log(n); flag_wait("open"); }`.
#[derive(Clone, Debug, Default)]
struct NoteThenFlag {
    waiting_note: bool,
}

impl Thread<Log> for NoteThenFlag {
    fn resume(&mut self, cx: &mut Cx<'_, Log>) -> Yield {
        if self.waiting_note {
            self.waiting_note = false;
            let note = cx.event().and_then(|event| event.args.first().cloned());
            cx.world.push(format!("{note:?}"));
            return Yield::FlagWait("open".into());
        }
        self.waiting_note = true;
        Yield::waittill(cx.owner(), "note")
    }
}

#[test]
fn a_set_flag_between_waittills_keeps_the_frames_notifies() {
    let owner = Owner(4);
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.flag_set("open");
    sched.spawn(owner, NoteThenFlag::default());
    run_frames(&mut sched, &mut log, 1);
    for n in [1, 2] {
        sched.notify(owner, "note", vec![Value::Int(n)]);
    }
    sched.run(FRAME_MS, &mut log);
    assert_eq!(log, ["Some(Int(1))", "Some(Int(2))"]);
}

#[test]
fn an_endon_in_the_frames_notifies_kills_the_waiter_before_the_earlier_ones() {
    // The port runs a woken waiter after the frame's notifies, so an endon
    // that follows its event still kills it first (the module doc's rule).
    const NOTE: &str = "note";
    let owner = Owner(5);
    let mut sched = Scheduler::default();
    let mut log = Log::new();
    sched.spawn(
        owner,
        WaitTillLog {
            owner,
            name: NOTE,
            endon: Some("death"),
            started: false,
        },
    );
    run_frames(&mut sched, &mut log, 1);
    sched.notify(owner, NOTE, vec![Value::Int(1)]);
    sched.notify(owner, "death", Vec::new());
    sched.run(FRAME_MS, &mut log);
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(sched.thread_count(), 0);
}
