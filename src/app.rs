use crate::{metrics::Metrics, types::Snapshot};

#[derive(Default)]
pub struct App {
    pub current: Option<Snapshot>,
    pub error: Option<String>,
    pub metrics: Metrics,
    pub tab: usize,
    pub help: bool,
}

impl App {
    pub fn apply(&mut self, result: Result<Snapshot, String>) {
        match result {
            Ok(s) => {
                self.metrics.update(&s);
                self.current = Some(s);
                self.error = None;
            }
            Err(message) => self.error = Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_retain_previous_data_until_a_valid_observation_recovers() {
        let mut app = App::default();
        app.apply(Ok(Snapshot::default()));
        app.apply(Err("Core is busy or unavailable".to_string()));
        assert!(app.current.is_some());
        assert!(app.error.is_some());
        app.apply(Ok(Snapshot::default()));
        assert!(app.error.is_none());
    }
}
