//! Interactive jobs find room in the queue while background jobs fill it (no starvation).
#![allow(clippy::unwrap_used)]

use zcie_runtime::JobQueue;
use zcie_types::{JobKind, JobPriority, JobRecord, Scope};

fn job(priority: JobPriority, index: usize) -> JobRecord {
    JobRecord::new(JobKind::IndexFile, Scope::default(), priority, index as u64, format!("wt:file{index}"))
}

#[test]
fn background_jobs_use_at_most_half_of_the_queue() {
    let queue = JobQueue::new(10);
    let accepted = (0..20).filter(|i| queue.push(job(JobPriority::Background, *i)).is_ok()).count();
    assert_eq!(accepted, 5);
    assert!(!queue.has_room_for(JobPriority::Background));
    assert!(queue.has_room_for(JobPriority::Interactive));
    let interactive = (100..120).filter(|i| queue.push(job(JobPriority::Interactive, *i)).is_ok()).count();
    assert_eq!(interactive, 5, "the rest of the capacity is for interactive jobs");
    assert!(!queue.has_room(1));
}

#[test]
fn pop_interactive_skips_background_jobs() {
    let queue = JobQueue::new(10);
    queue.push(job(JobPriority::Background, 1)).unwrap();
    assert!(queue.pop_interactive(std::time::Duration::from_millis(10)).is_none());
    queue.push(job(JobPriority::Interactive, 2)).unwrap();
    assert_eq!(queue.pop_interactive(std::time::Duration::from_millis(10)).unwrap().priority, JobPriority::Interactive);
    assert_eq!(queue.pop(std::time::Duration::from_millis(10)).unwrap().priority, JobPriority::Background);
}
