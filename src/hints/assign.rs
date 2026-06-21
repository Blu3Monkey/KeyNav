use crate::uia::ScannedElement;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct HintTarget {
    pub element: ScannedElement,
    pub label: String,
}

pub fn assign_hints(elements: Vec<ScannedElement>, alphabet: &[char]) -> Vec<HintTarget> {
    if elements.is_empty() {
        return Vec::new();
    }

    let labels = generate_labels(elements.len(), alphabet);
    elements
        .into_iter()
        .zip(labels)
        .map(|(element, label)| HintTarget { element, label })
        .collect()
}

/// Assign the shortest prefix-free labels over `alphabet` (BFS).
pub fn generate_labels(count: usize, alphabet: &[char]) -> Vec<String> {
    if alphabet.is_empty() || count == 0 {
        return Vec::new();
    }

    let alphabet: Vec<char> = alphabet.to_vec();
    let mut labels = Vec::with_capacity(count);
    let mut queue: VecDeque<String> = alphabet.iter().map(|&c| c.to_string()).collect();

    while labels.len() < count {
        let cand = queue
            .pop_front()
            .unwrap_or_else(|| panic!("alphabet too small for {count} hints"));

        if labels_conflict(&labels, &cand) {
            extend_queue(&mut queue, &cand, &alphabet);
            continue;
        }

        let remaining = count - labels.len();
        if remaining > 1 && cand.len() == 1 && remaining > available_single_slots(&labels, &alphabet)
        {
            extend_queue(&mut queue, &cand, &alphabet);
            continue;
        }

        labels.push(cand);
    }

    labels
}

fn labels_conflict(assigned: &[String], cand: &str) -> bool {
    assigned.iter().any(|a| {
        (cand.starts_with(a) && cand.len() > a.len())
            || (a.starts_with(cand) && a.len() > cand.len())
    })
}

fn available_single_slots(assigned: &[String], alphabet: &[char]) -> usize {
    alphabet
        .iter()
        .filter(|&&ch| {
            let s = ch.to_string();
            !assigned.iter().any(|a| labels_conflict(&[a.clone()], &s))
        })
        .count()
}

fn extend_queue(queue: &mut VecDeque<String>, prefix: &str, alphabet: &[char]) {
    for &ch in alphabet {
        queue.push_back(format!("{prefix}{ch}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_ALPHABET;

    fn default_alphabet_chars() -> Vec<char> {
        DEFAULT_ALPHABET.chars().collect()
    }

    fn no_prefix_conflicts(labels: &[String]) -> bool {
        for (i, a) in labels.iter().enumerate() {
            for (j, b) in labels.iter().enumerate() {
                if i != j && (a.starts_with(b) || b.starts_with(a)) {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn single_letter_labels_when_few_targets() {
        let alpha = default_alphabet_chars();
        let labels = generate_labels(5, &alpha);
        assert_eq!(labels, vec!["q", "w", "e", "r", "t"]);
        assert!(no_prefix_conflicts(&labels));
    }

    #[test]
    fn custom_alphabet_home_row() {
        let alpha: Vec<char> = "asdfghjkl".chars().collect();
        let labels = generate_labels(5, &alpha);
        assert_eq!(labels, vec!["a", "s", "d", "f", "g"]);
    }

    #[test]
    fn no_prefix_conflict_at_alphabet_boundary() {
        let alpha = default_alphabet_chars();
        let labels = generate_labels(alpha.len() + 1, &alpha);
        assert_eq!(labels.len(), alpha.len() + 1);
        assert!(no_prefix_conflicts(&labels));
    }

    #[test]
    fn no_prefix_conflict_with_many_targets() {
        let alpha = default_alphabet_chars();
        let labels = generate_labels(60, &alpha);
        assert_eq!(labels.len(), 60);
        assert!(no_prefix_conflicts(&labels));
        let has_e = labels.iter().any(|l| l == "e");
        let has_e_prefix_double = labels.iter().any(|l| l.len() > 1 && l.starts_with('e'));
        assert!(!(has_e && has_e_prefix_double));
    }
}
