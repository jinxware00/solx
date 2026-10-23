use crate::limits::{CancellationToken, ExecutionLimits, LimitKind};
use crate::runtime::RuntimeError;

pub(super) struct Control {
    pub limits: ExecutionLimits,
    pub cancellation: CancellationToken,
    remaining_steps: Option<u64>,
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
            depth: 0,
            calls: 0,
            imports: 0,
        }
    }

    pub fn begin_run(&mut self) -> Result<(), RuntimeError> {
        debug_assert_eq!((self.depth, self.calls, self.imports), (0, 0, 0));
        self.remaining_steps = self.limits.max_steps;
        self.check_cancelled()
    }

    pub fn check_cancelled(&self) -> Result<(), RuntimeError> {
        if self.cancellation.is_cancelled() {
            Err(RuntimeError::cancelled())
        } else {
            Ok(())
        }
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
