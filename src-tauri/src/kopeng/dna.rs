use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const FEMININE_BASE_IDS: &[usize] = &[5, 6, 11, 12];
const MASCULINE_BASE_IDS: &[usize] = &[1, 2, 3, 4, 7, 8, 9, 10, 13, 14];
const HAIR_COUNT: usize = 22; // H00..H21
const BEARD_COUNT: usize = 6; // BD00..BD05
const ACCESSORY_COUNT: usize = 17; // A00..A16

/// Generates a deterministic pixel portrait DNA string for an agent based on its ID.
/// Format: "B{:02}:E{}:H{:02}:BD{:02}:A{:02}" matching `public/portraits/manifest.json`.
pub fn generate_random_dna(agent_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    agent_id.hash(&mut hasher);
    let hash = hasher.finish();

    let is_feminine = (hash % 2) == 0;
    let base_idx = if is_feminine {
        FEMININE_BASE_IDS[((hash >> 2) as usize) % FEMININE_BASE_IDS.len()]
    } else {
        MASCULINE_BASE_IDS[((hash >> 2) as usize) % MASCULINE_BASE_IDS.len()]
    };

    let expr_idx = ((hash >> 6) % 6) as usize; // E0..E5
    let hair_idx = ((hash >> 10) as usize) % HAIR_COUNT; // H00..H21

    // Strict gender rule: Feminine base heads never receive beards (force BD00)
    let beard_idx = if is_feminine {
        0
    } else {
        ((hash >> 14) as usize) % BEARD_COUNT // BD00..BD05
    };

    let acc_idx = ((hash >> 18) as usize) % ACCESSORY_COUNT; // A00..A16

    format!(
        "B{:02}:E{}:H{:02}:BD{:02}:A{:02}",
        base_idx, expr_idx, hair_idx, beard_idx, acc_idx
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dna_generation_deterministic() {
        let dna1 = generate_random_dna("bosmang");
        let dna2 = generate_random_dna("bosmang");
        assert_eq!(dna1, dna2);
        assert!(dna1.starts_with('B'));
    }
}
