use crate::github::issues::IssueMetadata;

/// Calculate similarity score between two strings (0.0 to 1.0)
pub fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    let s1_lower = s1.to_lowercase();
    let s2_lower = s2.to_lowercase();

    // Direct substring match gets high score
    if s1_lower.contains(&s2_lower) || s2_lower.contains(&s1_lower) {
        return 0.8;
    }

    // Extract key error terms (remove common words)
    let stop_words = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
    ];

    let words1: Vec<&str> = s1_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stop_words.contains(w))
        .collect();

    let words2: Vec<&str> = s2_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stop_words.contains(w))
        .collect();

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

    if common == 0 {
        return 0.0;
    }

    // Calculate Jaccard similarity
    let total = words1.len() + words2.len() - common;
    let jaccard = if total > 0 {
        common as f64 / total as f64
    } else {
        0.0
    };

    // Boost score if many words match
    let match_ratio = common as f64 / words1.len().min(words2.len()) as f64;

    // Combine scores
    (jaccard * 0.6 + match_ratio * 0.4).min(1.0)
}

/// Find similar issues based on title and body
pub fn find_similar_issues(
    error_description: &str,
    issues: &[IssueMetadata],
    threshold: f64,
) -> Vec<(u64, f64)> {
    println!("Similarity matching:");
    println!("Query: '{}'", error_description);
    println!("Searching {} issues", issues.len());
    println!("Threshold: {:.2}", threshold);

    if issues.is_empty() {
        println!("No issues to search!");
        return Vec::new();
    }

    let mut all_scores: Vec<(u64, f64, String)> = Vec::new();

    for issue in issues {
        let title_sim = calculate_similarity(error_description, &issue.title);
        let body_sim = issue
            .body
            .as_ref()
            .map(|b| calculate_similarity(error_description, b))
            .unwrap_or(0.0);

        // Weight title much higher than body
        let score = (title_sim * 0.8) + (body_sim * 0.2);

        all_scores.push((issue.number, score, issue.title.clone()));
    }

    // Sort all by score to see what we're getting
    all_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Show top 5 matches regardless of threshold
    println!("matches:");
    for (num, score, title) in all_scores.iter().take(5) {
        let status = if *score >= threshold {
            "meets"
        } else {
            "fails"
        };
        println!("{} #{} ({:.2}): {}", status, num, score, title);
    }

    // Filter by threshold
    let similarities: Vec<(u64, f64)> = all_scores
        .into_iter()
        .filter(|(_, score, _)| *score >= threshold)
        .map(|(num, score, _)| (num, score))
        .collect();

    println!(
        "Result: {} matches above threshold {:.2}",
        similarities.len(),
        threshold
    );

    similarities
}
