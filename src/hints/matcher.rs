use super::HintTarget;

#[derive(Debug, Clone)]
pub enum MatchResult {
    /// Still narrowing; show only matching hints.
    Partial,
    /// Exactly one hint matches the typed prefix.
    Unique(HintTarget),
    /// No hints match.
    None,
}

#[derive(Debug, Default)]
pub struct HintMatcher {
    prefix: String,
}

impl HintMatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn push_char(&mut self, ch: char) -> MatchResult {
        self.prefix.push(ch);
        self.evaluate()
    }

    pub fn pop_char(&mut self) {
        self.prefix.pop();
    }

    pub fn clear(&mut self) {
        self.prefix.clear();
    }

    pub fn evaluate(&self) -> MatchResult {
        self.evaluate_against(&[])
    }

    pub fn evaluate_against(&self, targets: &[HintTarget]) -> MatchResult {
        if self.prefix.is_empty() {
            return MatchResult::Partial;
        }

        let matches: Vec<&HintTarget> = targets
            .iter()
            .filter(|t| t.label.starts_with(&self.prefix))
            .collect();

        match matches.len() {
            0 => MatchResult::None,
            1 if matches[0].label == self.prefix => MatchResult::Unique(matches[0].clone()),
            _ => MatchResult::Partial,
        }
    }

}
