use crate::github::issues::IssueMetadata;

/// Calculate similarity score between two strings (0.0 to 1.0)
pub fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    let s1_lower = s1.to_lowercase();
    let s2_lower = s2.to_lowercase();

    // Tokenize into words
    let words1: Vec<&str> = s1_lower.split_whitespace().collect();
    let words2: Vec<&str> = s2_lower.split_whitespace().collect();

    if words1.is_empty() || words2.is_empty() {
        return 0.0;
    }

    // Count common words
    let mut common = 0;
    for word in &words1 {
        if words2.contains(word) {
            common += 1;
        }
    }

    // Jaccard similarity
    let total = words1.len() + words2.len() - common;
    if total == 0 {
        return 0.0;
    }

    common as f64 / total as f64
}

/// Find similar closed issues based on title and body
pub fn find_similar_issues(
    error_description: &str,
    closed_issues: &[IssueMetadata],
    threshold: f64,
) -> Vec<(u64, f64)> {
    let mut similarities: Vec<(u64, f64)> = closed_issues
        .iter()
        .map(|issue| {
            let title_sim = calculate_similarity(error_description, &issue.title);
            let body_sim = issue
                .body
                .as_ref()
                .map(|b| calculate_similarity(error_description, b))
                .unwrap_or(0.0);

            // Weight title higher than body
            let score = (title_sim * 0.7) + (body_sim * 0.3);
            (issue.number, score)
        })
        .filter(|(_, score)| *score >= threshold)
        .collect();

    // Sort by similarity (highest first)
    similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    similarities
}
