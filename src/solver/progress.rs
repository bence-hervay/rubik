use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use super::SolvePhase;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct StageProgressSpec {
    pub phase: SolvePhase,
    pub stage: &'static str,
    pub total_work: usize,
    pub unit: &'static str,
}

impl StageProgressSpec {
    pub(crate) const fn new(
        phase: SolvePhase,
        stage: &'static str,
        total_work: usize,
        unit: &'static str,
    ) -> Self {
        Self {
            phase,
            stage,
            total_work,
            unit,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SolveProgress {
    Disabled,
    Enabled(ProgressState),
}

impl SolveProgress {
    pub(crate) const fn disabled() -> Self {
        Self::Disabled
    }

    pub(crate) fn enable(&mut self) {
        if matches!(self, Self::Disabled) {
            *self = Self::Enabled(ProgressState::default());
        }
    }

    pub(crate) const fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled(_))
    }

    pub(crate) fn start_stage(&mut self, spec: StageProgressSpec) {
        let Self::Enabled(state) = self else {
            return;
        };

        debug_assert!(
            state.active.is_none(),
            "a solve stage progress bar is already active",
        );
        if spec.total_work == 0 {
            return;
        }

        let bar = ProgressBar::new(spec.total_work as u64);
        bar.set_style(progress_style());
        bar.set_prefix(format!("{} [{}]", spec.stage, spec.phase));
        bar.set_message(spec.unit.to_owned());
        bar.enable_steady_tick(Duration::from_millis(120));

        state.active = Some(ActiveStageProgress {
            bar,
            total_work: spec.total_work as u64,
            completed_work: 0,
            rendered_work: 0,
            update_quantum: (spec.total_work as u64 / 512).max(1),
        });
    }

    pub(crate) fn advance(&mut self, delta: usize) {
        let Self::Enabled(state) = self else {
            return;
        };
        let Some(active) = state.active.as_mut() else {
            return;
        };

        active.completed_work = active
            .completed_work
            .saturating_add(delta as u64)
            .min(active.total_work);
        let should_render = active.completed_work == active.total_work
            || active.completed_work.saturating_sub(active.rendered_work) >= active.update_quantum;
        if should_render {
            active.rendered_work = active.completed_work;
            active.bar.set_position(active.rendered_work);
        }
    }

    pub(crate) fn finish_stage(&mut self) {
        let Self::Enabled(state) = self else {
            return;
        };
        let Some(active) = state.active.take() else {
            return;
        };

        active.bar.set_position(active.total_work);
        active.bar.finish_and_clear();
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProgressState {
    active: Option<ActiveStageProgress>,
}

#[derive(Clone, Debug)]
struct ActiveStageProgress {
    bar: ProgressBar,
    total_work: u64,
    completed_work: u64,
    rendered_work: u64,
    update_quantum: u64,
}

fn progress_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.cyan} {prefix:<28} [{bar:40.cyan/blue}] {pos:>9}/{len:9} {percent:>3}% {elapsed_precise}<{eta_precise}",
    )
    .expect("progress template must be valid")
    .progress_chars("=> ")
}
