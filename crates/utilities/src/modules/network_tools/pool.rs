//! Exécution en parallèle de nombreuses petites tâches (une par machine ou par port).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// Lance `job(0..total)` sur `workers` fils d'exécution.
///
/// `tick` est appelé environ dix fois par seconde avec le nombre de tâches terminées,
/// depuis le fil appelant : il peut donc mettre à jour une barre de progression
/// sans être `Sync`.
pub fn run_parallel<J>(total: usize, workers: usize, job: J, mut tick: impl FnMut(usize))
where
    J: Fn(usize) + Sync,
{
    if total == 0 {
        return;
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.clamp(1, total))
            .map(|_| {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= total {
                        break;
                    }
                    job(index);
                    done.fetch_add(1, Ordering::Relaxed);
                })
            })
            .collect();
        while !handles.iter().all(|handle| handle.is_finished()) {
            tick(done.load(Ordering::Relaxed));
            thread::sleep(Duration::from_millis(100));
        }
        tick(done.load(Ordering::Relaxed));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn every_job_runs_exactly_once() {
        let seen = Mutex::new(vec![0u32; 500]);
        let mut last_tick = 0;
        run_parallel(
            500,
            16,
            |i| seen.lock().unwrap()[i] += 1,
            |done| last_tick = done,
        );
        assert!(seen.lock().unwrap().iter().all(|&n| n == 1));
        assert_eq!(last_tick, 500);
    }

    #[test]
    fn zero_jobs_is_fine() {
        run_parallel(0, 8, |_| unreachable!(), |_| unreachable!());
    }
}
