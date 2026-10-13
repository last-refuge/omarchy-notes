//! Optional native-only queue fairness diagnostic.
//!
//! The performance caller must retain `QueueProbe`, pin the observed synthetic
//! editor through a weak entity/id, and supply guarded native typing separately.
//! This module neither draws nor generates input. It queues real GPUI foreground
//! runnables, whose Linux dispatcher uses calloop idle callbacks. Synthetic busy
//! work is explicitly reported; it does not model indexing/import costs.
use anyhow::{Result, ensure};
use gpui::{App, Task};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    hint::spin_loop,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

pub(super) type Progress = Rc<dyn Fn(&App) -> Option<u64>>;
const MAX_EDIT_PROGRESS_EVENTS: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub(super) struct QueueProbeConfig {
    pub warmup: Duration,
    pub burst_interval: Duration,
    pub bursts: u32,
    pub callbacks_per_burst: u32,
    pub callback_work: Duration,
    pub heartbeat_interval: Duration,
    pub cooldown: Duration,
}

impl Default for QueueProbeConfig {
    fn default() -> Self {
        Self {
            warmup: Duration::from_secs(1),
            burst_interval: Duration::from_millis(250),
            bursts: 40,
            callbacks_per_burst: 64,
            callback_work: Duration::from_micros(500),
            heartbeat_interval: Duration::from_millis(5),
            cooldown: Duration::from_secs(1),
        }
    }
}

impl QueueProbeConfig {
    fn validate(self) -> Result<Duration> {
        ensure!((1..=120).contains(&self.bursts), "Require 1–120 bursts");
        ensure!(
            self.callbacks_per_burst <= 128,
            "At most 128 callbacks per burst"
        );
        ensure!(
            self.bursts * self.callbacks_per_burst <= 8192,
            "At most 8,192 retained callback samples"
        );
        ensure!(
            self.callback_work <= Duration::from_millis(2),
            "At most 2 ms work per callback"
        );
        let batch_work = self.callback_work * self.callbacks_per_burst;
        ensure!(
            batch_work <= Duration::from_millis(100),
            "At most 100 ms requested work per burst"
        );
        ensure!(
            self.burst_interval >= Duration::from_millis(100)
                && self.burst_interval <= Duration::from_secs(1)
                && self.burst_interval >= batch_work * 2,
            "Bursts must leave a bounded recovery interval"
        );
        ensure!(
            (Duration::from_millis(2)..=Duration::from_millis(50))
                .contains(&self.heartbeat_interval),
            "Heartbeat interval must be 2–50 ms"
        );
        ensure!(
            (Duration::from_millis(100)..=Duration::from_secs(5)).contains(&self.warmup),
            "Require a bounded passive warmup"
        );
        ensure!(
            (Duration::from_millis(100)..=Duration::from_secs(5)).contains(&self.cooldown),
            "Require a bounded passive cooldown"
        );
        let duration = self.warmup + self.burst_interval * self.bursts + self.cooldown;
        ensure!(
            duration <= Duration::from_secs(30),
            "Probe duration is bounded to 30 s"
        );
        Ok(duration)
    }
}

struct CallbackSample {
    burst: u32,
    ordinal: u32,
    started: Duration,
    queued_for: Duration,
    execution: Duration,
}

struct HeartbeatSample {
    due: Duration,
    started: Duration,
    timer_lateness: Duration,
    foreground_delay: Duration,
    service_gap: Duration,
    progress_read: Duration,
    revision: Option<u64>,
}

struct EditProgressSample {
    delivered: Duration,
    revision: u64,
    advanced: bool,
    callbacks_enqueued: u32,
    callbacks_completed: u32,
    latest_callback: Option<(u32, u32)>,
}

#[derive(Default)]
struct EditProgressCounts {
    deliveries: u64,
    revision_advances: u64,
    duplicate_revisions: u64,
    backward_revisions: u64,
    pending_callback_deliveries: u64,
    pending_callback_revision_advances: u64,
    dropped_at_cap: u64,
    ignored_after_duration: u64,
    ignored_cancelled: u64,
}

struct ProbeState {
    active: bool,
    driver_complete: bool,
    heartbeat_complete: bool,
    enqueued: u32,
    skipped_heartbeat_ticks: u64,
    burst_lateness: Vec<Duration>,
    callbacks: Vec<CallbackSample>,
    heartbeats: Vec<HeartbeatSample>,
    initial_revision: Option<u64>,
    latest_edit_revision: Option<u64>,
    edits: Vec<EditProgressSample>,
    edit_counts: EditProgressCounts,
}

/// A weak recorder for one retained Changed subscription on the fixed editor.
/// It contains no entity/window handle and cannot keep the probe alive.
#[derive(Clone)]
pub(super) struct EditProgressRecorder {
    state: Weak<RefCell<ProbeState>>,
    started: Instant,
    deadline: Instant,
}

impl EditProgressRecorder {
    /// O(1). This timestamp is Changed-subscriber delivery, not native dispatch
    /// arrival, key-send time or frame submission. No text is read or retained.
    pub(super) fn record(&self, revision: u64) {
        let delivered = Instant::now();
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut state = state.borrow_mut();
        if !state.active {
            state.edit_counts.ignored_cancelled =
                state.edit_counts.ignored_cancelled.saturating_add(1);
            return;
        }
        if delivered > self.deadline {
            state.edit_counts.ignored_after_duration =
                state.edit_counts.ignored_after_duration.saturating_add(1);
            return;
        }
        let previous = state.latest_edit_revision;
        let advanced = previous.is_some_and(|previous| revision > previous);
        let duplicate = previous == Some(revision);
        let backward = previous.is_some_and(|previous| revision < previous);
        let completed = state.callbacks.len() as u32;
        let enqueued = state.enqueued;
        let pending = enqueued > completed;
        state.latest_edit_revision = Some(revision);
        state.edit_counts.deliveries = state.edit_counts.deliveries.saturating_add(1);
        state.edit_counts.revision_advances += u64::from(advanced);
        state.edit_counts.duplicate_revisions += u64::from(duplicate);
        state.edit_counts.backward_revisions += u64::from(backward);
        state.edit_counts.pending_callback_deliveries += u64::from(pending);
        state.edit_counts.pending_callback_revision_advances += u64::from(pending && advanced);
        if state.edits.len() == MAX_EDIT_PROGRESS_EVENTS {
            state.edit_counts.dropped_at_cap = state.edit_counts.dropped_at_cap.saturating_add(1);
            return;
        }
        let latest_callback = state
            .callbacks
            .last()
            .map(|sample| (sample.burst, sample.ordinal));
        state.edits.push(EditProgressSample {
            delivered: delivered.duration_since(self.started),
            revision,
            advanced,
            callbacks_enqueued: enqueued,
            callbacks_completed: completed,
            latest_callback,
        });
    }
}

/// Owns all tasks. Dropping the probe cancels its drivers and queued callbacks.
pub(super) struct QueueProbe {
    config: QueueProbeConfig,
    started: Instant,
    duration: Duration,
    state: Rc<RefCell<ProbeState>>,
    _driver: Task<()>,
    _heartbeat: Task<()>,
}

impl QueueProbe {
    /// `progress` must read one pinned synthetic editor's revision in O(1).
    /// Returning `None` records loss of that target; no other editor is selected.
    /// Setting callbacks_per_burst=0 supplies the matched passive control.
    pub(super) fn start(
        config: QueueProbeConfig,
        progress: Progress,
        cx: &mut App,
    ) -> Result<Self> {
        let duration = config.validate()?;
        ensure!(
            !cx.background_executor().is_test(),
            "Queue fairness requires the native event loop"
        );
        let started = Instant::now();
        let deadline = started + duration;
        let initial_revision = progress(cx);
        let state = Rc::new(RefCell::new(ProbeState {
            active: true,
            driver_complete: false,
            heartbeat_complete: false,
            enqueued: 0,
            skipped_heartbeat_ticks: 0,
            burst_lateness: Vec::with_capacity(config.bursts as usize),
            callbacks: Vec::with_capacity((config.bursts * config.callbacks_per_burst) as usize),
            heartbeats: Vec::with_capacity(
                (duration.as_millis() / config.heartbeat_interval.as_millis()) as usize + 1,
            ),
            initial_revision,
            latest_edit_revision: initial_revision,
            edits: Vec::with_capacity(MAX_EDIT_PROGRESS_EVENTS),
            edit_counts: EditProgressCounts::default(),
        }));
        let driver_state = state.clone();
        let driver = cx.spawn(async move |cx| {
            // Handles stay in this future, not in the callback state: no Rc cycle.
            let mut tasks =
                Vec::with_capacity((config.bursts * config.callbacks_per_burst) as usize);
            for burst in 0..config.bursts {
                let due = started + config.warmup + config.burst_interval * burst;
                cx.background_executor()
                    .timer(due.saturating_duration_since(Instant::now()))
                    .await;
                if !driver_state.borrow().active || Instant::now() >= deadline {
                    break;
                }
                driver_state
                    .borrow_mut()
                    .burst_lateness
                    .push(Instant::now().saturating_duration_since(due));
                cx.update(|cx| {
                    for ordinal in 0..config.callbacks_per_burst {
                        let queued = Instant::now();
                        let callback_state = driver_state.clone();
                        tasks.push(cx.spawn(async move |_| {
                            if !callback_state.borrow().active {
                                return;
                            }
                            let callback_started = Instant::now();
                            let stop = callback_started + config.callback_work;
                            while Instant::now() < stop {
                                spin_loop();
                            }
                            let execution = callback_started.elapsed();
                            callback_state.borrow_mut().callbacks.push(CallbackSample {
                                burst,
                                ordinal,
                                started: callback_started.duration_since(started),
                                queued_for: callback_started.duration_since(queued),
                                execution,
                            });
                        }));
                        driver_state.borrow_mut().enqueued += 1;
                    }
                });
            }
            for task in tasks {
                task.await;
            }
            driver_state.borrow_mut().driver_complete = true;
        });
        let heartbeat_state = state.clone();
        let heartbeat = cx.spawn(async move |cx| {
            let mut due = started + config.heartbeat_interval;
            let mut previous = started;
            loop {
                if due > deadline || !heartbeat_state.borrow().active {
                    break;
                }
                let background = cx.background_executor().clone();
                // Timestamp the background wake separately. A delayed timer or
                // busy worker pool must not be mistaken for foreground queuing.
                let woke = cx
                    .background_executor()
                    .spawn(async move {
                        background
                            .timer(due.saturating_duration_since(Instant::now()))
                            .await;
                        Instant::now()
                    })
                    .await;
                let serviced = Instant::now();
                if !heartbeat_state.borrow().active {
                    break;
                }
                let revision = cx.update(|cx| progress(cx));
                let progress_read = serviced.elapsed();
                let mut state = heartbeat_state.borrow_mut();
                state.heartbeats.push(HeartbeatSample {
                    due: due.duration_since(started),
                    started: serviced.duration_since(started),
                    timer_lateness: woke.saturating_duration_since(due),
                    foreground_delay: serviced.duration_since(woke),
                    service_gap: serviced.duration_since(previous),
                    progress_read,
                    revision,
                });
                if revision.is_none() {
                    state.active = false;
                }
                drop(state);
                previous = serviced;
                due += config.heartbeat_interval;
                if due <= serviced {
                    // Do not flood the recovering queue with missed heartbeats.
                    let skipped = (serviced.duration_since(due).as_nanos()
                        / config.heartbeat_interval.as_nanos()
                        + 1) as u32;
                    heartbeat_state.borrow_mut().skipped_heartbeat_ticks += u64::from(skipped);
                    due += config.heartbeat_interval * skipped;
                }
            }
            heartbeat_state.borrow_mut().heartbeat_complete = true;
        });
        Ok(Self {
            config,
            started,
            duration,
            state,
            _driver: driver,
            _heartbeat: heartbeat,
        })
    }

    /// Exact monotonic epoch shared by callbacks, heartbeat and caller journals.
    pub(super) fn started_at(&self) -> Instant {
        self.started
    }

    pub(super) fn edit_recorder(&self) -> EditProgressRecorder {
        EditProgressRecorder {
            state: Rc::downgrade(&self.state),
            started: self.started,
            deadline: self.started + self.duration,
        }
    }

    /// Planned relative phase boundaries, for the caller's native histogram
    /// captures. The caller should also record each capture's actual timestamp.
    pub(super) fn phase_offsets(&self) -> (Duration, Duration, Duration) {
        (
            self.config.warmup,
            self.config.warmup + self.config.burst_interval * self.config.bursts,
            self.duration,
        )
    }

    /// O(1) periodic metadata: does not walk, clone or sort either sample vector.
    pub(super) fn status(&self) -> Value {
        let state = self.state.borrow();
        let elapsed = self.started.elapsed();
        let (load_start, load_end, end) = self.phase_offsets();
        let finished = state.driver_complete && state.heartbeat_complete;
        let phase = if !state.active {
            "cancelled_missing_target"
        } else if finished {
            "complete"
        } else if elapsed < load_start {
            "warmup"
        } else if elapsed < load_end {
            "load"
        } else if elapsed < end {
            "cooldown"
        } else {
            "awaiting_completion"
        };
        json!({
            "phase":phase,
            "elapsed_ms":millis(elapsed),
            "planned_load_start_ms":millis(load_start),
            "planned_load_end_ms":millis(load_end),
            "planned_end_ms":millis(end),
            "synthetic_load_enabled":self.config.callbacks_per_burst != 0,
            "requested_callbacks":self.config.bursts*self.config.callbacks_per_burst,
            "enqueued_callbacks":state.enqueued,
            "completed_callbacks":state.callbacks.len(),
            "pending_callbacks":(state.enqueued as usize).saturating_sub(state.callbacks.len()),
            "heartbeat_samples":state.heartbeats.len(),
            "latest_revision":state.heartbeats.last().and_then(|s|s.revision),
            "changed_deliveries":state.edit_counts.deliveries,
            "changed_raw_samples":state.edits.len(),
            "changed_raw_dropped_at_cap":state.edit_counts.dropped_at_cap,
            "target_lost":!state.active,
            "driver_finished":state.driver_complete,
            "heartbeat_finished":state.heartbeat_complete,
        })
    }

    /// Snapshot once after cooldown, before dropping; sorting/reporting is not
    /// part of a timed callback. Raw relative timestamps allow burst attribution.
    pub(super) fn snapshot(&self) -> Value {
        let state = self.state.borrow();
        let load_end = self.config.warmup + self.config.burst_interval * self.config.bursts;
        let mut previous = None;
        let mut first = None;
        let mut last = None;
        let mut changes = 0;
        let mut regressions = 0;
        let mut missing = 0;
        for sample in &state.heartbeats {
            match sample.revision {
                Some(revision) => {
                    first.get_or_insert(revision);
                    if let Some(prior) = previous {
                        changes += u64::from(revision != prior);
                        regressions += u64::from(revision < prior);
                    }
                    previous = Some(revision);
                    last = Some(revision);
                }
                None => missing += 1,
            }
        }
        let phase = |samples: Vec<&HeartbeatSample>| {
            json!({
                "samples": samples.len(),
                "foreground_queue_delay": distribution(samples.iter().map(|s| s.foreground_delay)),
                "background_timer_lateness": distribution(samples.iter().map(|s| s.timer_lateness)),
                "foreground_service_gap": distribution(samples.iter().map(|s| s.service_gap)),
                "progress_read": distribution(samples.iter().map(|s| s.progress_read)),
            })
        };
        let burst_progress: Vec<_> = (0..self.config.bursts).map(|burst| {
            let callbacks: Vec<_> = state.callbacks.iter().filter(|s|s.burst==burst).collect();
            let queued = callbacks.iter().map(|s|s.started.saturating_sub(s.queued_for)).min();
            let first_started = callbacks.iter().map(|s|s.started).min();
            let last_finished = callbacks.iter().map(|s|s.started+s.execution).max();
            let between_callbacks = |sample: &&EditProgressSample| {
                sample.callbacks_enqueued > sample.callbacks_completed
                    && first_started.zip(last_finished).is_some_and(|(a,b)|sample.delivered>=a&&sample.delivered<b)
            };
            let interleaved = state.edits.iter().filter(between_callbacks).count();
            let advancing = state.edits.iter().filter(|s|s.advanced).filter(between_callbacks).count();
            json!({"burst":burst,"completed_callbacks":callbacks.len(),"first_queued_ms":queued.map(millis),"first_callback_ms":first_started.map(millis),"last_callback_finished_ms":last_finished.map(millis),"changed_deliveries_between_callbacks":interleaved,"revision_advances_between_callbacks":advancing})
        }).collect();
        let counts = &state.edit_counts;
        let edit_progress = json!({
            "scope":"Changed-subscriber delivery on one fixed synthetic editor during probe epoch/deadline; not key-send time, native event arrival or frame submission",
            "record_limit":MAX_EDIT_PROGRESS_EVENTS,
            "records":state.edits.len(),
            "raw_truncated":counts.dropped_at_cap != 0,
            "dropped_at_cap":counts.dropped_at_cap,
            "ignored_after_duration":counts.ignored_after_duration,
            "ignored_cancelled":counts.ignored_cancelled,
            "deliveries":counts.deliveries,
            "revision_advances":counts.revision_advances,
            "duplicate_revision_deliveries":counts.duplicate_revisions,
            "backward_revision_deliveries":counts.backward_revisions,
            "initial_revision":state.initial_revision,
            "latest_revision":state.latest_edit_revision,
            "revision_delta":state.initial_revision.zip(state.latest_edit_revision).filter(|_|counts.backward_revisions==0).map(|(a,b)|b.saturating_sub(a)),
            "deliveries_with_pending_callbacks":counts.pending_callback_deliveries,
            "revision_advances_with_pending_callbacks":counts.pending_callback_revision_advances,
            "bursts_with_interleaved_revision_advances":burst_progress.iter().filter(|b|b["revision_advances_between_callbacks"].as_u64().unwrap_or(0)>0).count(),
            "burst_attribution_scope":"Retained raw delivery timestamps between the first start and last finish of each burst, with pending callbacks at delivery. Counts are incomplete if the raw record cap overflowed. Bursts can overlap under extreme stalls.",
            "bursts":burst_progress,
            "raw_columns":["delivered_ms","revision","advanced","enqueued_callbacks","completed_callbacks","pending_callbacks","last_completed_burst","last_completed_ordinal"],
            "raw":state.edits.iter().map(|s|json!([millis(s.delivered),s.revision,s.advanced,s.callbacks_enqueued,s.callbacks_completed,s.callbacks_enqueued.saturating_sub(s.callbacks_completed),s.latest_callback.map(|(b,_)|b),s.latest_callback.map(|(_,o)|o)])).collect::<Vec<_>>(),
        });
        json!({
            "method": "native GPUI ready foreground callbacks plus background-timer-woken foreground heartbeat; synthetic busy work",
            "complete": state.driver_complete && state.heartbeat_complete && state.callbacks.len() == (self.config.bursts*self.config.callbacks_per_burst) as usize && missing == 0,
            "driver_finished": state.driver_complete,
            "heartbeat_finished": state.heartbeat_complete,
            "requested_duration_ms": millis(self.duration),
            "last_heartbeat_service_ms": state.heartbeats.last().map(|s|millis(s.started)),
            "config": {"warmup_ms":millis(self.config.warmup),"burst_interval_ms":millis(self.config.burst_interval),"bursts":self.config.bursts,"callbacks_per_burst":self.config.callbacks_per_burst,"callback_work_ms":millis(self.config.callback_work),"heartbeat_interval_ms":millis(self.config.heartbeat_interval),"cooldown_ms":millis(self.config.cooldown)},
            "callbacks": {"requested":self.config.bursts*self.config.callbacks_per_burst,"enqueued":state.enqueued,"completed":state.callbacks.len(),"enqueue_lateness":distribution(state.burst_lateness.iter().copied()),"queue_delay":distribution(state.callbacks.iter().map(|s|s.queued_for)),"execution":distribution(state.callbacks.iter().map(|s|s.execution))},
            "heartbeat": {"skipped_ticks":state.skipped_heartbeat_ticks,"passive_warmup":phase(state.heartbeats.iter().filter(|s|s.due<self.config.warmup).collect()),"load":phase(state.heartbeats.iter().filter(|s|s.due>=self.config.warmup&&s.due<load_end).collect()),"passive_cooldown":phase(state.heartbeats.iter().filter(|s|s.due>=load_end).collect())},
            "observed_editor_progress": {"first_revision":first,"last_revision":last,"revision_delta":first.zip(last).filter(|_|regressions==0).map(|(a,b)|b.saturating_sub(a)),"sampled_revision_changes":changes,"backward_revision_samples":regressions,"missing_target_samples":missing,"scope":"one caller-pinned synthetic editor; input source and latency come from the separately guarded native workload"},
            "edit_progress":edit_progress,
            "callback_raw_columns": ["burst","ordinal","start_ms","queue_ms","execution_ms"],
            "callback_raw":state.callbacks.iter().map(|s|json!([s.burst,s.ordinal,millis(s.started),millis(s.queued_for),millis(s.execution)])).collect::<Vec<_>>(),
            "heartbeat_raw_columns": ["due_ms","start_ms","timer_lateness_ms","foreground_delay_ms","service_gap_ms","revision"],
            "heartbeat_raw":state.heartbeats.iter().map(|s|json!([millis(s.due),millis(s.started),millis(s.timer_lateness),millis(s.foreground_delay),millis(s.service_gap),s.revision])).collect::<Vec<_>>(),
            "acceptance": "diagnostic evidence only; no automatic calloop decision, display-presentation claim, or indexing/import workload coverage",
        })
    }
}

impl Drop for QueueProbe {
    fn drop(&mut self) {
        self.state.borrow_mut().active = false;
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.
}

fn distribution(values: impl Iterator<Item = Duration>) -> Value {
    let mut sorted: Vec<_> = values.map(millis).collect();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        (!sorted.is_empty())
            .then(|| sorted[((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1)])
    };
    json!({"samples":sorted.len(),"p50_ms":percentile(0.50),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),"longest_ms":sorted.last().copied(),"total_ms":sorted.iter().sum::<f64>()})
}
