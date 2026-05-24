use std::sync::OnceLock;
use std::time::{Duration, Instant};

const PERF_ENV_VAR: &str = "SOUKOU_PERF_PASTE";
const LOG_THRESHOLD: Duration = Duration::from_millis(1);

pub(crate) fn paste_perf_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(PERF_ENV_VAR).is_some())
}

pub(crate) fn log_paste_perf<F>(label: &str, detail: F, elapsed: Duration)
where
    F: FnOnce() -> String,
{
    if !paste_perf_enabled() || elapsed < LOG_THRESHOLD {
        return;
    }

    eprintln!(
        "[perf][paste] {label}: {:.2}ms {}",
        elapsed.as_secs_f64() * 1000.0,
        detail()
    );
}

pub(crate) struct PerfScope<F>
where
    F: FnOnce(Duration),
{
    start: Instant,
    on_drop: Option<F>,
}

impl<F> PerfScope<F>
where
    F: FnOnce(Duration),
{
    pub(crate) fn new(on_drop: F) -> Self {
        Self {
            start: Instant::now(),
            on_drop: Some(on_drop),
        }
    }
}

impl<F> Drop for PerfScope<F>
where
    F: FnOnce(Duration),
{
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop(self.start.elapsed());
        }
    }
}
