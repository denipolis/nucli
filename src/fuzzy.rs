pub fn score(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).filter(|c| !c.is_whitespace()).collect();
    let chars: Vec<char> = candidate.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    if query.len() > lower.len() {
        return None;
    }
    let bonus = |index: usize| -> i32 {
        if index == 0 {
            return 16;
        }
        let previous = chars[index - 1];
        let current = chars[index];
        if matches!(previous, '/' | '\\' | '_' | '-' | ' ' | '.' | ':') {
            14
        } else if previous.is_lowercase() && current.is_uppercase() {
            12
        } else {
            0
        }
    };
    let rows = query.len();
    let cols = lower.len();
    let unset = i32::MIN / 2;
    let mut best = vec![vec![unset; cols]; rows];
    let mut from = vec![vec![usize::MAX; cols]; rows];
    for i in 0..rows {
        let mut running = unset;
        let mut running_index = usize::MAX;
        for j in 0..cols {
            if i > 0 && j >= 2 && best[i - 1][j - 2] > unset {
                let candidate = best[i - 1][j - 2] + (j as i32 - 2);
                if candidate > running {
                    running = candidate;
                    running_index = j - 2;
                }
            }
            if lower[j] != query[i] {
                continue;
            }
            let base = 16 + bonus(j);
            if i == 0 {
                best[i][j] = base - j as i32 / 4;
                continue;
            }
            let consecutive = if j >= 1 && best[i - 1][j - 1] > unset { best[i - 1][j - 1] + base + 8 } else { unset };
            let gap = if running > unset { running - (j as i32 - 1) + base } else { unset };
            if consecutive > unset && consecutive >= gap {
                best[i][j] = consecutive;
                from[i][j] = j - 1;
            } else if gap > unset {
                best[i][j] = gap;
                from[i][j] = running_index;
            }
        }
    }
    let last = rows - 1;
    let (end, total) =
        best[last].iter().enumerate().filter(|(_, value)| **value > unset).max_by_key(|(_, value)| **value)?;
    let mut positions = vec![0; rows];
    let mut column = end;
    for row in (0..rows).rev() {
        positions[row] = column;
        if row > 0 {
            column = from[row][column];
        }
    }
    Some((*total - cols as i32 / 8, positions))
}

pub fn score_path(query: &str, path: &str) -> Option<(i32, Vec<usize>)> {
    let name_start = path.rfind(['/', '\\']).map(|index| index + 1).unwrap_or(0);
    let name_offset = path[..name_start].chars().count();
    let by_name = score(query, &path[name_start..])
        .map(|(score, positions)| (score + 40, positions.into_iter().map(|index| index + name_offset).collect()));
    let full = score(query, path);
    match (by_name, full) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_matches() {
        assert!(score("xyz", "abc").is_none());
        let (_, positions) = score("nt", "New Tab").unwrap();
        assert_eq!(positions, vec![0, 4]);
        let word = score("nt", "New Tab").unwrap().0;
        let middle = score("nt", "printer").unwrap().0;
        assert!(word > middle);
        let file = score_path("main", "src/main.rs").unwrap().0;
        let folder = score_path("main", "main/other.rs").unwrap().0;
        assert!(file > folder);
        assert!(score("ab", "a_b").unwrap().0 > score("ab", "axxxb").unwrap().0);
    }
}
