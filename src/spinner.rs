use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct EngineSpinner {
    pb: ProgressBar,
}

impl Default for EngineSpinner {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineSpinner {
    pub fn new() -> Self {
        let pb = ProgressBar::new_spinner();

        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.green} {msg}")
                .expect("Valid static layout format template"),
        );

        pb.enable_steady_tick(Duration::from_millis(100));

        Self { pb }
    }

    pub fn set_message(&self, msg: String) {
        self.pb.set_message(msg);
    }

    pub fn finish_success(&self) {
        self.pb
            .finish_with_message("Autonomous loop execution completed.");
    }
}
