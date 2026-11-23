// SPDX-License-Identifier: GPL-3.0-only

//! Pending-command counts and session write timings for the storage worker.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

use super::worker::Command;

#[derive(Default)]
pub(super) struct QueueMetrics {
    pub(super) queued: AtomicUsize,
    pub(super) peak_queued: AtomicUsize,
    pub(super) session_writes: AtomicU64,
    pub(super) worst_session_latency_ns: AtomicU64,
    pub(super) report: bool,
}

#[derive(Clone, Copy, Debug, Default)]
#[cfg(test)]
pub(crate) struct QueueMetricsSnapshot {
    pub(crate) queued: usize,
    pub(crate) peak_queued: usize,
    pub(crate) session_writes: u64,
    pub(crate) worst_session_latency: std::time::Duration,
}

impl QueueMetrics {
    pub(super) fn enqueue(&self, sender: &Sender<Command>, command: Command) -> Result<(), ()> {
        let queued = self.queued.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_queued.fetch_max(queued, Ordering::Relaxed);
        if sender.send(command).is_err() {
            self.queued.fetch_sub(1, Ordering::Relaxed);
            return Err(());
        }
        Ok(())
    }

    pub(super) fn dequeued(&self) {
        self.queued.fetch_sub(1, Ordering::Relaxed);
    }

    pub(super) fn session_write_finished(&self, queued_at: std::time::Instant) {
        let writes = self.session_writes.fetch_add(1, Ordering::Relaxed) + 1;
        let latency = queued_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        self.worst_session_latency_ns
            .fetch_max(latency, Ordering::Relaxed);
        if self.report {
            eprintln!(
                "Elaris storage: session_write_attempts={writes} queued={} peak_queued={} worst_save_ms={:.2}",
                self.queued.load(Ordering::Relaxed),
                self.peak_queued.load(Ordering::Relaxed),
                self.worst_session_latency_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            );
        }
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> QueueMetricsSnapshot {
        QueueMetricsSnapshot {
            queued: self.queued.load(Ordering::Relaxed),
            peak_queued: self.peak_queued.load(Ordering::Relaxed),
            session_writes: self.session_writes.load(Ordering::Relaxed),
            worst_session_latency: std::time::Duration::from_nanos(
                self.worst_session_latency_ns.load(Ordering::Relaxed),
            ),
        }
    }
}
