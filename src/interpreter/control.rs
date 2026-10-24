use crate::limits::{CancellationToken, ExecutionLimits, LimitKind};
use crate::runtime::RuntimeError;
use std::time::{Duration, Instant};

pub(super) struct Control {
    pub limits: ExecutionLimits,
    pub cancellation: CancellationToken,
    remaining_steps: Option<u64>,
    remaining_format_work: u64,
    elapsed_budget: Option<(Instant, Duration)>,
    depth: usize,
    calls: usize,
    imports: usize,
}

impl Control {
    pub fn new(limits: ExecutionLimits) -> Self {
        Self {
            limits,
            cancellation: CancellationToken::default(),
            remaining_steps: limits.max_steps,
            remaining_format_work: limits.max_format_work,
            elapsed_budget: None,
            depth: 0,
            calls: 0,
            imports: 0,
        }
    }

    pub fn begin_run(&mut self) -> Result<(), RuntimeError> {
        debug_assert_eq!((self.depth, self.calls, self.imports), (0, 0, 0));
        self.remaining_steps = self.limits.max_steps;
        self.remaining_format_work = self.limits.max_format_work;
        self.elapsed_budget = self
            .limits
            .max_duration
            .map(|duration| (Instant::now(), duration));
        self.check_cancelled()
    }

    pub fn check_cancelled(&self) -> Result<(), RuntimeError> {
        if self.cancellation.is_cancelled() {
            return Err(RuntimeError::cancelled());
        }
        // Reuse all existing cooperative cancellation checkpoints. Disabled
        // deadlines avoid reading the clock on the ordinary execution path.
        if self.elapsed_budget.is_some() {
            self.check_deadline(Instant::now())?;
        }
        Ok(())
    }

    fn check_deadline(&self, now: Instant) -> Result<(), RuntimeError> {
        if let Some((started, duration)) = self.elapsed_budget
            && now.saturating_duration_since(started) >= duration
        {
            return Err(RuntimeError::limit(
                LimitKind::Deadline,
                format!("{} ms", duration.as_millis()),
            ));
        }
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), RuntimeError> {
        self.check_cancelled()?;
        if let Some(remaining) = &mut self.remaining_steps {
            if *remaining == 0 {
                return Err(RuntimeError::limit(
                    LimitKind::Steps,
                    self.limits.max_steps.unwrap(),
                ));
            }
            *remaining -= 1;
        }
        Ok(())
    }

    pub fn format_work(&mut self, units: usize) -> Result<(), RuntimeError> {
        self.check_cancelled()?;
        let units = u64::try_from(units).unwrap_or(u64::MAX);
        self.remaining_format_work =
            self.remaining_format_work
                .checked_sub(units)
                .ok_or_else(|| {
                    RuntimeError::limit(LimitKind::FormatWork, self.limits.max_format_work)
                })?;
        Ok(())
    }

    pub fn enter_evaluation(&mut self) -> Result<(), RuntimeError> {
        self.step()?;
        enter(
            &mut self.depth,
            self.limits.max_evaluation_depth,
            LimitKind::EvaluationDepth,
        )
    }

    pub fn leave_evaluation(&mut self) {
        self.depth -= 1;
    }

    pub fn enter_call(&mut self) -> Result<(), RuntimeError> {
        enter(
            &mut self.calls,
            self.limits.max_call_depth,
            LimitKind::CallDepth,
        )
    }

    pub fn leave_call(&mut self) {
        self.calls -= 1;
    }

    pub fn enter_import(&mut self) -> Result<(), RuntimeError> {
        self.check_cancelled()?;
        enter(
            &mut self.imports,
            self.limits.max_import_depth,
            LimitKind::ImportDepth,
        )
    }

    pub fn leave_import(&mut self) {
        self.imports -= 1;
    }
}

fn enter(depth: &mut usize, limit: usize, kind: LimitKind) -> Result<(), RuntimeError> {
    if *depth >= limit {
        return Err(RuntimeError::limit(kind, limit));
    }
    *depth += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::error::RuntimeErrorKind;

    #[test]
    fn deadline_has_an_inclusive_monotonic_boundary_without_sleeping() {
        let duration = Duration::from_secs(10);
        let mut control = Control::new(ExecutionLimits {
            max_duration: Some(duration),
            ..ExecutionLimits::default()
        });
        control.begin_run().unwrap();
        let started = control.elapsed_budget.unwrap().0;
        control
            .check_deadline(started + duration - Duration::from_nanos(1))
            .unwrap();
        let error = control.check_deadline(started + duration).unwrap_err();
        assert_eq!(
            error.kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)
        );
        assert!(!error.is_catchable());
        assert_eq!(
            error.message,
            "execution deadline limit exceeded (10000 ms)"
        );
    }

    #[test]
    fn calls_and_imports_share_deadline_while_public_runs_reset_it() {
        let mut control = Control::new(ExecutionLimits {
            max_duration: Some(Duration::from_secs(3600)),
            ..ExecutionLimits::default()
        });
        control.begin_run().unwrap();
        let budget = control.elapsed_budget;
        control.enter_call().unwrap();
        control.enter_import().unwrap();
        control.enter_evaluation().unwrap();
        assert_eq!(control.elapsed_budget, budget);
        control.leave_evaluation();
        control.leave_import();
        control.leave_call();
        // Force expiration deterministically, then begin a fresh public run.
        control.elapsed_budget.as_mut().unwrap().1 = Duration::ZERO;
        assert!(control.check_cancelled().is_err());
        control.begin_run().unwrap();
        assert_eq!(control.elapsed_budget.unwrap().1, Duration::from_secs(3600));
    }

    #[test]
    fn disabled_deadlines_do_not_create_a_timer_and_large_durations_do_not_overflow() {
        let mut disabled = Control::new(ExecutionLimits::default());
        disabled.begin_run().unwrap();
        assert!(disabled.elapsed_budget.is_none());
        disabled.step().unwrap();
        let mut huge = Control::new(ExecutionLimits {
            max_duration: Some(Duration::MAX),
            ..ExecutionLimits::default()
        });
        huge.begin_run().unwrap();
        huge.check_cancelled().unwrap();
    }

    #[test]
    fn cancellation_remains_sticky_and_takes_precedence_over_an_expired_deadline() {
        let mut control = Control::new(ExecutionLimits {
            max_duration: Some(Duration::ZERO),
            ..ExecutionLimits::default()
        });
        control.cancellation.cancel();
        assert_eq!(
            control.begin_run().unwrap_err().kind,
            RuntimeErrorKind::Cancelled
        );
        control.cancellation.reset();
        assert_eq!(
            control.begin_run().unwrap_err().kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)
        );
    }

    #[test]
    fn format_work_checks_interruptions_without_consuming_evaluation_steps() {
        let mut control = Control::new(ExecutionLimits {
            max_steps: Some(1),
            max_format_work: 2,
            max_duration: Some(Duration::from_secs(3600)),
            ..ExecutionLimits::default()
        });
        control.begin_run().unwrap();
        control.format_work(2).unwrap();
        control.step().unwrap();
        assert_eq!(
            control.format_work(1).unwrap_err().kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::FormatWork)
        );
        control.begin_run().unwrap();
        control.format_work(2).unwrap();
        control.elapsed_budget.as_mut().unwrap().1 = Duration::ZERO;
        assert_eq!(
            control.format_work(0).unwrap_err().kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)
        );
        control.cancellation.cancel();
        assert_eq!(
            control.format_work(0).unwrap_err().kind,
            RuntimeErrorKind::Cancelled
        );
    }
}
