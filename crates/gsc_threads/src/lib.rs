//! GSC thread semantics for scripts ported to Rust.
//!
//! A GSC function that waits (`wait`, `waittill`, `flag_wait`) becomes a
//! [`Thread`]: a small state machine whose [`Thread::resume`] runs until the
//! next wait and returns it as a [`Yield`]. The [`Scheduler`] owns the threads,
//! wakes them on time, on `notify` and on flags, and kills them on `endon`.
//!
//! It is deterministic: threads run in the order they became ready, ties in
//! thread-id order, and nothing reads a clock or iterates a hash map. It is
//! `Clone`, so a whole simulation holding it can be copied for prediction and
//! replay.
//!
//! Two orderings differ from the retail VM, which runs a spawned thread and a
//! woken waiter *inside* the `thread`/`notify` call: here both run later in the
//! same frame, after the thread that caused them yields. A script that depends
//! on that interleaving within one frame has to say so where it is ported.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

/// Whose thread or event this is: `level`, or an entity the game names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(pub u64);

impl Owner {
    pub const LEVEL: Self = Self(0);
}

/// An event or flag name. GSC names are strings and scripts build them at run
/// time (`"zone_" + name`), so they are owned, cheap to clone and ordered.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(Arc<str>);

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for Name {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

/// A notify argument, as `waittill("damage", amount, attacker)` receives it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Undefined,
    Int(i64),
    Float(f32),
    Str(Name),
    Owner(Owner),
    Vec3([f32; 3]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub owner: Owner,
    pub name: Name,
    pub args: Vec<Value>,
}

/// Where a thread stops until something wakes it.
#[derive(Clone, Debug, PartialEq)]
pub enum Yield {
    /// `wait <seconds>`, in milliseconds. `wait 0.05` is one server frame.
    Wait { ms: u64 },
    /// `waittillframeend` / `wait 0.05` when the frame is what matters.
    WaitFrame,
    /// `owner waittill(name)`; several names is `waittill_any`.
    WaitTill { owner: Owner, names: Vec<Name> },
    /// `flag_wait(name)`: returns at once when the flag is already set.
    FlagWait(Name),
    /// `flag_waitopen(name)`: returns at once when the flag is clear.
    FlagWaitClear(Name),
    /// The function returned.
    Done,
}

impl Yield {
    pub fn wait_seconds(seconds: f32) -> Self {
        Self::Wait {
            ms: (seconds * 1000.0).round().max(0.0) as u64,
        }
    }

    pub fn waittill(owner: Owner, name: impl Into<Name>) -> Self {
        Self::WaitTill {
            owner,
            names: vec![name.into()],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

/// A plain call of a function that waits (`chalk_one_up();` without
/// `thread`): runs `callee` in the caller's thread. `Some(wait)` means the
/// callee is waiting and the caller must return that wait; `None` means it
/// returned and the caller carries on.
pub fn call<W>(callee: &mut dyn Thread<W>, cx: &mut Cx<'_, W>) -> Option<Yield> {
    match callee.resume(cx) {
        Yield::Done => None,
        waiting => Some(waiting),
    }
}

/// Lets `Box<dyn Thread<W>>` be cloned; implemented for every `Clone` thread.
pub trait ThreadClone<W> {
    fn clone_box(&self) -> Box<dyn Thread<W>>;
}

impl<W, T> ThreadClone<W> for T
where
    T: Thread<W> + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn Thread<W>> {
        Box::new(self.clone())
    }
}

/// One ported GSC function that can wait. `W` is what the script works on:
/// the game-side context it reads and changes.
pub trait Thread<W>: ThreadClone<W> + fmt::Debug + Send + Sync {
    fn resume(&mut self, cx: &mut Cx<'_, W>) -> Yield;
}

enum Op<W> {
    Spawn(Owner, Box<dyn Thread<W>>),
    Notify(Event),
    Endon(Owner, Name),
    FlagSet(Name),
    FlagClear(Name),
}

/// What a running thread sees: the world, the time, the event that woke it,
/// and the GSC statements that act on other threads.
pub struct Cx<'a, W> {
    pub world: &'a mut W,
    now_ms: u64,
    this: ThreadId,
    owner: Owner,
    woke_by: Option<Event>,
    flags: &'a BTreeMap<Name, bool>,
    ops: Vec<Op<W>>,
}

impl<W> Cx<'_, W> {
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    pub fn this(&self) -> ThreadId {
        self.this
    }

    /// `self` in the GSC function.
    pub fn owner(&self) -> Owner {
        self.owner
    }

    /// The event that ended the last `waittill`, with its arguments.
    pub fn event(&self) -> Option<&Event> {
        self.woke_by.as_ref()
    }

    /// `owner thread f()`. The new thread runs later this frame.
    pub fn thread(&mut self, owner: Owner, thread: impl Thread<W> + 'static) {
        self.ops.push(Op::Spawn(owner, Box::new(thread)));
    }

    /// `owner notify(name, args…)`.
    pub fn notify(&mut self, owner: Owner, name: impl Into<Name>, args: Vec<Value>) {
        self.ops.push(Op::Notify(Event {
            owner,
            name: name.into(),
            args,
        }));
    }

    /// `owner endon(name)` for the running thread.
    pub fn endon(&mut self, owner: Owner, name: impl Into<Name>) {
        self.ops.push(Op::Endon(owner, name.into()));
    }

    /// `flag(name)`, as the flags stood when this thread was resumed.
    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    /// `flag_set(name)`.
    pub fn flag_set(&mut self, name: impl Into<Name>) {
        self.ops.push(Op::FlagSet(name.into()));
    }

    /// `flag_clear(name)`.
    pub fn flag_clear(&mut self, name: impl Into<Name>) {
        self.ops.push(Op::FlagClear(name.into()));
    }
}

impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Park {
    Ready,
    Until(u64),
    NextFrame,
    Till { owner: Owner, names: Vec<Name> },
    Flag(Name),
    FlagClear(Name),
}

struct Slot<W> {
    thread: Box<dyn Thread<W>>,
    owner: Owner,
    park: Park,
    endon: Vec<(Owner, Name)>,
    woke_by: Option<Event>,
}

impl<W> Clone for Slot<W> {
    fn clone(&self) -> Self {
        Self {
            thread: self.thread.clone_box(),
            owner: self.owner,
            park: self.park.clone(),
            endon: self.endon.clone(),
            woke_by: self.woke_by.clone(),
        }
    }
}

/// What one frame did, for reports and for catching a runaway script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunReport {
    pub resumed: u32,
    pub spawned: u32,
    pub killed: u32,
    /// The frame hit [`Scheduler::MAX_RESUMES_PER_FRAME`] and stopped early:
    /// some thread keeps waking itself without waiting.
    pub runaway: bool,
}

pub struct Scheduler<W> {
    threads: BTreeMap<ThreadId, Slot<W>>,
    flags: BTreeMap<Name, bool>,
    next_id: u64,
    ready: VecDeque<ThreadId>,
}

impl<W> Default for Scheduler<W> {
    fn default() -> Self {
        Self {
            threads: BTreeMap::new(),
            flags: BTreeMap::new(),
            next_id: 1,
            ready: VecDeque::new(),
        }
    }
}

impl<W> Clone for Scheduler<W> {
    fn clone(&self) -> Self {
        Self {
            threads: self.threads.clone(),
            flags: self.flags.clone(),
            next_id: self.next_id,
            ready: self.ready.clone(),
        }
    }
}

impl<W> fmt::Debug for Scheduler<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("threads", &self.threads.len())
            .field("flags", &self.flags)
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl<W> Scheduler<W> {
    pub const MAX_RESUMES_PER_FRAME: u32 = 100_000;

    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    /// `flag_init(name)`: declares the flag, clear.
    pub fn flag_init(&mut self, name: impl Into<Name>) {
        self.flags.entry(name.into()).or_insert(false);
    }

    /// `flag_set(name)` from the game side; wakes `flag_wait`ers.
    pub fn flag_set(&mut self, name: impl Into<Name>) {
        self.set_flag(name.into(), true);
    }

    /// `flag_clear(name)` from the game side; wakes `flag_waitopen`ers.
    pub fn flag_clear(&mut self, name: impl Into<Name>) {
        self.set_flag(name.into(), false);
    }

    /// Starts a thread from outside a script (the game calling `main()`). It
    /// runs on the next [`Scheduler::run`].
    pub fn spawn(&mut self, owner: Owner, thread: impl Thread<W> + 'static) -> ThreadId {
        self.insert(owner, Box::new(thread))
    }

    /// A notify from the game itself (damage, death, trigger): kills its
    /// `endon`s now and wakes its waiters for the next [`Scheduler::run`].
    pub fn notify(&mut self, owner: Owner, name: impl Into<Name>, args: Vec<Value>) -> u32 {
        self.deliver(Event {
            owner,
            name: name.into(),
            args,
        })
    }

    /// Kills every thread whose `self` is `owner`, as freeing an entity does.
    pub fn kill_owner(&mut self, owner: Owner) -> u32 {
        let before = self.threads.len();
        self.threads.retain(|_, slot| slot.owner != owner);
        (before - self.threads.len()) as u32
    }

    /// Runs one server frame at `now_ms`: wakes what is due, then resumes every
    /// ready thread, including the ones woken or spawned during the frame.
    pub fn run(&mut self, now_ms: u64, world: &mut W) -> RunReport {
        let mut report = RunReport::default();
        let due: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, slot)| match slot.park {
                Park::Until(at) => at <= now_ms,
                Park::NextFrame => true,
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.make_ready(id);
        }
        while let Some(id) = self.ready.pop_front() {
            if report.resumed >= Self::MAX_RESUMES_PER_FRAME {
                self.ready.push_front(id);
                report.runaway = true;
                break;
            }
            let Some(mut slot) = self.threads.remove(&id) else {
                continue;
            };
            if slot.park != Park::Ready {
                self.threads.insert(id, slot);
                continue;
            }
            let mut cx = Cx {
                world: &mut *world,
                now_ms,
                this: id,
                owner: slot.owner,
                woke_by: slot.woke_by.take(),
                flags: &self.flags,
                ops: Vec::new(),
            };
            let yielded = slot.thread.resume(&mut cx);
            let ops = cx.ops;
            report.resumed += 1;
            self.threads.insert(id, slot);
            for op in ops {
                match op {
                    Op::Spawn(owner, thread) => {
                        self.insert(owner, thread);
                        report.spawned += 1;
                    }
                    Op::Notify(event) => report.killed += self.deliver(event),
                    Op::Endon(owner, name) => {
                        if let Some(slot) = self.threads.get_mut(&id) {
                            slot.endon.push((owner, name));
                        }
                    }
                    Op::FlagSet(name) => self.set_flag(name, true),
                    Op::FlagClear(name) => self.set_flag(name, false),
                }
            }
            if self.threads.contains_key(&id) {
                self.park(id, yielded, now_ms);
            }
        }
        report
    }

    fn insert(&mut self, owner: Owner, thread: Box<dyn Thread<W>>) -> ThreadId {
        let id = ThreadId(self.next_id);
        self.next_id += 1;
        self.threads.insert(
            id,
            Slot {
                thread,
                owner,
                park: Park::Ready,
                endon: Vec::new(),
                woke_by: None,
            },
        );
        self.ready.push_back(id);
        id
    }

    fn make_ready(&mut self, id: ThreadId) {
        if let Some(slot) = self.threads.get_mut(&id)
            && slot.park != Park::Ready
        {
            slot.park = Park::Ready;
            self.ready.push_back(id);
        }
    }

    fn park(&mut self, id: ThreadId, yielded: Yield, now_ms: u64) {
        let park = match yielded {
            Yield::Done => {
                self.threads.remove(&id);
                return;
            }
            Yield::Wait { ms: 0 } | Yield::WaitFrame => Park::NextFrame,
            Yield::Wait { ms } => Park::Until(now_ms + ms),
            Yield::WaitTill { owner, names } => Park::Till { owner, names },
            Yield::FlagWait(name) if self.flag(name.as_str()) => Park::Ready,
            Yield::FlagWait(name) => Park::Flag(name),
            Yield::FlagWaitClear(name) if !self.flag(name.as_str()) => Park::Ready,
            Yield::FlagWaitClear(name) => Park::FlagClear(name),
        };
        let Some(slot) = self.threads.get_mut(&id) else {
            return;
        };
        let now_ready = park == Park::Ready;
        slot.park = park;
        if now_ready {
            self.ready.push_back(id);
        }
    }

    fn deliver(&mut self, event: Event) -> u32 {
        let before = self.threads.len();
        self.threads.retain(|_, slot| {
            !slot
                .endon
                .iter()
                .any(|(owner, name)| *owner == event.owner && *name == event.name)
        });
        let killed = (before - self.threads.len()) as u32;
        let woken: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, slot)| match &slot.park {
                Park::Till { owner, names } => *owner == event.owner && names.contains(&event.name),
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in woken {
            if let Some(slot) = self.threads.get_mut(&id) {
                slot.woke_by = Some(event.clone());
            }
            self.make_ready(id);
        }
        killed
    }

    fn set_flag(&mut self, name: Name, value: bool) {
        let woken: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, slot)| match &slot.park {
                Park::Flag(want) => value && *want == name,
                Park::FlagClear(want) => !value && *want == name,
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        self.flags.insert(name, value);
        for id in woken {
            self.make_ready(id);
        }
    }
}
