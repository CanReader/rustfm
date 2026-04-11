/// Minimal subsequence fuzzy matcher. Returns (score, indices) where higher is better.
/// Scoring favours consecutive matches, matches at word boundaries, and matches near the start.
pub fn score(needle: &str, haystack: &str) -> Option<(i32, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }
    let needle: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    let hay: Vec<char> = haystack.chars().collect();
    let hay_lower: Vec<char> = hay.iter().flat_map(|c| c.to_lowercase()).collect();

    let mut indices = Vec::with_capacity(needle.len());
    let mut ni = 0;
    let mut last_match: Option<usize> = None;
    let mut score: i32 = 0;

    for (hi, hc) in hay_lower.iter().enumerate() {
        if ni >= needle.len() {
            break;
        }
        if *hc == needle[ni] {
            indices.push(hi);
            // base award
            score += 10;
            if let Some(last) = last_match {
                if hi == last + 1 {
                    score += 15; // consecutive bonus
                }
            } else if hi == 0 {
                score += 20; // start of string
            }
            if hi > 0 {
                let prev = hay[hi - 1];
                if !prev.is_alphanumeric() {
                    score += 8; // word boundary
                }
            }
            last_match = Some(hi);
            ni += 1;
        }
    }
    if ni < needle.len() {
        return None;
    }
    // penalise long haystacks slightly
    score -= (hay.len() as i32) / 4;
    Some((score, indices))
}
