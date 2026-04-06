mod jenkins;

pub use jenkins::jenkins_hash;

use rand::Rng;
use std::collections::HashSet;

/// Result of perfect hash generation for a set of keys.
///
/// For any key in the original set:
/// ```text
/// index = (g[jenkins_hash(seed1, key) % g.len()]
///        + g[jenkins_hash(seed2, key) % g.len()]) % num_keys
/// ```
#[derive(Debug, Clone)]
pub struct PerfectHash {
    pub seed1: u32,
    pub seed2: u32,
    pub g_table: Vec<i32>,
    pub num_keys: usize,
}

impl PerfectHash {
    /// Look up the storage index for a key.
    pub fn lookup(&self, key: u32) -> usize {
        if self.num_keys == 0 {
            return 0;
        }
        let n = self.g_table.len();
        let h1 = jenkins_hash(self.seed1, key) as usize % n;
        let h2 = jenkins_hash(self.seed2, key) as usize % n;
        // Use i64 to avoid overflow on i32 addition
        let sum = self.g_table[h1] as i64 + self.g_table[h2] as i64;
        sum.rem_euclid(self.num_keys as i64) as usize
    }

    /// Verify that all keys map to unique indices in [0, num_keys).
    pub fn verify(&self, keys: &[u32]) -> bool {
        let mut seen = HashSet::new();
        for &key in keys {
            if !seen.insert(self.lookup(key)) {
                return false;
            }
        }
        seen.len() == self.num_keys
    }
}

/// Generate a minimal perfect hash function for the given set of keys.
///
/// Uses the CHM (Czech-Havas-Majewski) algorithm:
/// 1. Pick two random hash seeds
/// 2. Build an undirected graph where each key creates an edge between
///    two vertices (determined by hashing with seed1 and seed2)
/// 3. If the graph is acyclic, assign edge values via DFS to build the G table
/// 4. If cyclic, retry with new seeds
///
/// Returns `None` if generation fails after `max_iters` attempts.
pub fn generate(keys: &[u32], max_iters: u32) -> Option<PerfectHash> {
    if keys.is_empty() {
        return Some(PerfectHash {
            seed1: 0,
            seed2: 0,
            g_table: vec![],
            num_keys: 0,
        });
    }

    // Deduplicate
    let unique: Vec<u32> = {
        let mut set = HashSet::new();
        keys.iter().filter(|&&k| set.insert(k)).copied().collect()
    };
    let num_keys = unique.len();

    if num_keys == 1 {
        return Some(PerfectHash {
            seed1: 0,
            seed2: 0,
            g_table: vec![0],
            num_keys: 1,
        });
    }

    // Graph size: ceil(2.09 * num_keys)
    let n = ((2.09 * num_keys as f64).ceil()) as usize;
    let mut rng = rand::rng();

    for _ in 0..max_iters {
        let seed1: u32 = rng.random();
        let seed2: u32 = rng.random();

        if let Some(g_table) = try_build(&unique, n, seed1, seed2) {
            let ph = PerfectHash {
                seed1,
                seed2,
                g_table,
                num_keys,
            };
            if ph.verify(&unique) {
                return Some(ph);
            }
        }
    }

    None
}

/// Generate with specific seeds (for deterministic testing).
pub fn generate_with_seeds(keys: &[u32], seed1: u32, seed2: u32) -> Option<PerfectHash> {
    if keys.is_empty() {
        return Some(PerfectHash {
            seed1,
            seed2,
            g_table: vec![],
            num_keys: 0,
        });
    }

    let unique: Vec<u32> = {
        let mut set = HashSet::new();
        keys.iter().filter(|&&k| set.insert(k)).copied().collect()
    };
    let num_keys = unique.len();
    let n = ((2.09 * num_keys as f64).ceil()) as usize;

    let g_table = try_build(&unique, n, seed1, seed2)?;
    let ph = PerfectHash {
        seed1,
        seed2,
        g_table,
        num_keys,
    };
    if ph.verify(&unique) {
        Some(ph)
    } else {
        None
    }
}

/// Attempt to build the G table for given seeds.
///
/// Constructs an undirected graph and checks acyclicity. If acyclic,
/// performs DFS to assign G values such that for each edge (u, v) with
/// sequential DFS edge_id, G[u] + G[v] = edge_id.
///
/// Returns None if the graph has cycles or self-loops.
fn try_build(keys: &[u32], n: usize, seed1: u32, seed2: u32) -> Option<Vec<i32>> {
    // Adjacency list: each entry is (neighbor_vertex)
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];

    // Build edges: one per key
    for &key in keys {
        let h1 = jenkins_hash(seed1, key) as usize % n;
        let h2 = jenkins_hash(seed2, key) as usize % n;

        if h1 == h2 {
            return None; // Self-loop
        }

        adj[h1].push(h2);
        adj[h2].push(h1);
    }

    // DFS to check acyclicity and assign G values.
    // In the CHM algorithm, G[v] = edge_id - G[u] for each DFS tree edge,
    // where edge_id is a global counter incrementing across all components.
    let mut g = vec![0i32; n];
    let mut visited = vec![false; n];
    let mut edge_id: i32 = 0;

    for start in 0..n {
        if visited[start] || adj[start].is_empty() {
            continue;
        }

        // Iterative DFS
        visited[start] = true;
        let mut stack: Vec<usize> = vec![start];

        while let Some(u) = stack.pop() {
            // Process neighbors in order (matching NetworkX insertion-order DFS)
            // We need to iterate in order, but stack reverses, so push in reverse
            let neighbors: Vec<usize> = adj[u].iter().copied().filter(|&v| !visited[v]).collect();

            for &v in &neighbors {
                visited[v] = true;
                g[v] = edge_id - g[u];
                edge_id += 1;
                stack.push(v);
            }
        }

        // Check if we consumed the right number of edges.
        // In a tree with k vertices, there are k-1 edges.
        // If there were cycles, some vertices would have been skipped.
    }

    // Verify edge count: in a forest with E edges and V visited vertices,
    // edges = visited - components. But easier to just verify the hash works.
    // We let the caller verify.

    // Check: total edges assigned should equal num_keys
    if edge_id as usize != keys.len() {
        return None; // Graph had cycles (some edges were back-edges, skipped)
    }

    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_keyset() {
        let ph = generate(&[], 100).unwrap();
        assert_eq!(ph.num_keys, 0);
    }

    #[test]
    fn single_key() {
        let ph = generate(&[42], 100).unwrap();
        assert_eq!(ph.num_keys, 1);
        assert_eq!(ph.lookup(42), 0);
    }

    #[test]
    fn known_seeds_produce_valid_hash() {
        // Seeds from Python reference: seed1=0xa3b1799d, seed2=0x1c80317f
        // The G table may differ from Python (different DFS order) but
        // the result must still be a valid minimal perfect hash.
        let keys: Vec<u32> = vec![1, 2, 3, 4, 5, 0x10, 0x20, 0x100, 0x1000, 0x10000];

        let ph = generate_with_seeds(&keys, 0xa3b1799d, 0x1c80317f).unwrap();

        // G table size must match: ceil(2.09 * 10) = 21
        assert_eq!(ph.g_table.len(), 21);
        assert_eq!(ph.num_keys, 10);

        // All keys must map to unique indices in [0, 10)
        assert!(ph.verify(&keys));
        for &k in &keys {
            assert!(ph.lookup(k) < 10);
        }
    }

    #[test]
    fn generate_small_set() {
        let keys: Vec<u32> = vec![10, 20, 30, 40, 50];
        let ph = generate(&keys, 100).unwrap();
        assert_eq!(ph.num_keys, 5);
        assert!(ph.verify(&keys));

        // All indices should be in [0, 5)
        for &k in &keys {
            assert!(ph.lookup(k) < 5);
        }
    }

    #[test]
    fn generate_medium_set() {
        let keys: Vec<u32> = (1..=100).collect();
        let ph = generate(&keys, 100).unwrap();
        assert_eq!(ph.num_keys, 100);
        assert!(ph.verify(&keys));
    }

    #[test]
    fn generate_larger_set() {
        // Simulate a real-world key set (~500 keys)
        let keys: Vec<u32> = (0..500).map(|i| i * 0x1234 + 0xABCD0000).collect();
        let ph = generate(&keys, 100).unwrap();
        assert_eq!(ph.num_keys, 500);
        assert!(ph.verify(&keys));
    }

    #[test]
    fn deduplicates_input() {
        let keys = vec![1, 2, 3, 1, 2, 3];
        let ph = generate(&keys, 100).unwrap();
        assert_eq!(ph.num_keys, 3);
        assert!(ph.verify(&[1, 2, 3]));
    }

    #[test]
    fn g_table_size() {
        let keys: Vec<u32> = (1..=50).collect();
        let ph = generate(&keys, 100).unwrap();
        // G table should be ceil(2.09 * 50) = 105
        assert_eq!(ph.g_table.len(), 105);
    }
}
