// progress.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;
use std::sync::atomic::{Ordering, AtomicUsize};

pub struct ProgressReporter {
    desc: String,
    start_time: Instant,
    interval: AtomicUsize,
    counter: AtomicUsize,
    total: usize
}

pub struct ProgressHandle<'a> {
    reporter: &'a ProgressReporter,
    last_report: Instant,
    interval: usize,
    local_counter: usize
}

impl<'a> Drop for ProgressHandle<'a> {
    fn drop(&mut self) {
        self.reporter.report_up(self.local_counter);
    }
}

impl<'a> Drop for ProgressReporter {
    fn drop(&mut self) {
        println!("[{}] Completed {} in {}ms", self.desc, self.counter.load(Ordering::SeqCst),
            self.start_time.elapsed().as_millis());
    }
}

impl<'a> ProgressHandle<'a> {
    pub fn report(&mut self, increment: usize) {
        self.local_counter += increment;
        if self.local_counter >= self.interval {
            self.push();
        }
    }

    #[cold]
    fn push(&mut self) {
        let elapsed = self.last_report.elapsed();
        let elapsed = elapsed.as_secs() * 1000 + (elapsed.subsec_millis() as u64);
        let mut ratio = 1.0 / (elapsed as f64);
        if ratio < 0.25 {
            ratio = 0.25;
        } else if ratio > 4.0 {
            ratio = 4.0;
        }

        self.interval = ((self.interval) as f64 * ratio) as usize;
        self.reporter.report_up(self.local_counter);
        self.local_counter = 0;
        self.last_report = Instant::now();
    }
}

impl ProgressReporter {
    pub fn handle<'a>(&'a self) -> ProgressHandle<'a> {
        ProgressHandle { reporter: self, last_report: Instant::now(), interval: 10000, local_counter: 0 }
    }

    pub fn new(desc: &str, total: usize) -> Self {
        ProgressReporter {
            desc: String::from(desc),
            start_time: Instant::now(),
            interval: AtomicUsize::new(1000),
            counter: AtomicUsize::new(0),
            total
        }
    }

    fn report_up(&self, count: usize) {
        let interval = self.interval.load(Ordering::Relaxed);
        let prior = self.counter.fetch_add(count, Ordering::Relaxed);
        let newval = prior + count;

        if prior / interval != newval / interval {
            self.display(interval);
        }
    }

    fn display(&self, old_interval: usize) {
        let curval = self.counter.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed();
        let elapsed = elapsed.as_secs() * 1000 + (elapsed.subsec_millis() as u64);
        let rate = (curval as f64) / (elapsed as f64) * 1000.0;

        let mut new_interval = rate as usize;
        if new_interval > old_interval * 4 {
            new_interval = old_interval * 4;
        }
        if new_interval < 100 {
            new_interval = 100;
        }

        self.interval.compare_and_swap(old_interval, new_interval, Ordering::Relaxed);

        println!("[{}] {} ({}/s, {}s remain)",
            self.desc, curval, rate, ((self.total - curval) as f64) / rate
        );
    }
}

