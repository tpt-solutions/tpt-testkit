//! Shrinking crashing inputs, grouping crashes by signature, and minimizing corpora.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{self, AssertUnwindSafe};

use super::{Failure, fnv1a};

/// Shrinks `input` while `still_fails` keeps returning true, and returns the smallest result.
///
/// This is delta debugging: remove chunks of halving size, then single bytes, until no removal
/// keeps the failure. The result is 1-minimal: removing any single byte makes it pass. If the
/// original input does not fail, it is returned unchanged.
/// # Examples
/// ```
/// use tpt_testkit::fuzz::minimize;
///
/// let shrunk = minimize(b"abcXdef", |data| data.contains(&b'X'));
/// assert_eq!(shrunk, b"X");
/// ```
pub fn minimize(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut current = input.to_vec();
    if !still_fails(&current) {
        return current;
    }
    let mut chunk = current.len().div_ceil(2).max(1);
    loop {
        let mut progressed = false;
        let mut start = 0;
        while start < current.len() {
            let end = (start + chunk).min(current.len());
            let mut candidate = current[..start].to_vec();
            candidate.extend_from_slice(&current[end..]);
            if still_fails(&candidate) {
                current = candidate;
                progressed = true;
            } else {
                start += chunk;
            }
        }
        if chunk > 1 {
            chunk = chunk.div_ceil(2);
        } else if !progressed {
            break;
        }
    }
    current
}

/// Shrinks a crashing input: [`minimize`] with "the target panics" as the predicate.
///
/// Panic output from each attempt is printed by the default hook.
pub fn minimize_crash<F>(input: &[u8], target: F) -> Vec<u8>
where
    F: Fn(&[u8]),
{
    minimize(input, |data| {
        panic::catch_unwind(AssertUnwindSafe(|| target(data))).is_err()
    })
}

/// Stable signature for a panic message.
///
/// Digit runs and hex literals are collapsed to `#`, so messages that differ only in indices,
/// addresses or counts get the same signature. The signature is FNV-1a over the normalized text.
pub fn signature(message: &str) -> u64 {
    fnv1a(normalize(message).as_bytes())
}

fn normalize(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            while matches!(chars.peek(), Some(n) if n.is_ascii_hexdigit() || *n == 'x' || *n == 'X')
            {
                chars.next();
            }
            out.push('#');
        } else {
            out.push(c);
        }
    }
    out
}

/// Crashes that share a signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashGroup {
    /// Signature from [`signature`].
    pub signature: u64,
    /// The message of the first crash seen in this group.
    pub message: String,
    /// How many crashes fell into this group.
    pub count: usize,
    /// The shortest input in the group. Ties are broken by byte order.
    pub smallest: Vec<u8>,
}

/// Groups failures by [`signature`]. Groups are returned sorted by signature.
pub fn dedup(failures: &[Failure]) -> Vec<CrashGroup> {
    let mut groups: BTreeMap<u64, CrashGroup> = BTreeMap::new();
    for failure in failures {
        let sig = signature(&failure.message);
        let group = groups.entry(sig).or_insert_with(|| CrashGroup {
            signature: sig,
            message: failure.message.clone(),
            count: 0,
            smallest: failure.input.clone(),
        });
        group.count += 1;
        if (failure.input.len(), &failure.input) < (group.smallest.len(), &group.smallest) {
            group.smallest.clone_from(&failure.input);
        }
    }
    groups.into_values().collect()
}

/// Groups whose signature is not in `known`. Use it to report only new crashes in CI.
pub fn unknown_groups<'a>(groups: &'a [CrashGroup], known: &[u64]) -> Vec<&'a CrashGroup> {
    groups
        .iter()
        .filter(|group| !known.contains(&group.signature))
        .collect()
}

/// Keeps the smallest set of inputs that together report every feature any input reports.
///
/// Inputs are considered shortest first, so the kept set favors small inputs. An input is kept
/// only if it reports a feature no earlier kept input reported. Inputs that report no features
/// are dropped.
pub fn minimize_corpus<F>(inputs: &[Vec<u8>], features: F) -> Vec<Vec<u8>>
where
    F: Fn(&[u8]) -> Vec<u64>,
{
    let mut order: Vec<&Vec<u8>> = inputs.iter().collect();
    order.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    order.dedup();

    let mut covered = BTreeSet::new();
    let mut kept = Vec::new();
    for input in order {
        let reported = features(input);
        if reported.iter().any(|feature| !covered.contains(feature)) {
            covered.extend(reported);
            kept.push(input.clone());
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimize_reaches_one_minimal_input() {
        let input = b"aaXbbbYcccc";
        let shrunk = minimize(input, |data| data.contains(&b'X') && data.contains(&b'Y'));
        assert_eq!(shrunk, b"XY");
    }

    #[test]
    fn minimize_returns_passing_input_unchanged() {
        assert_eq!(minimize(b"fine", |_| false), b"fine");
    }

    #[test]
    fn minimize_crash_shrinks_to_trigger() {
        let shrunk = minimize_crash(b"..........!!..........", |data| {
            assert!(!data.contains(&b'!'), "bang");
        });
        assert_eq!(shrunk, b"!");
    }

    #[test]
    fn signatures_ignore_numbers_and_addresses() {
        assert_eq!(
            signature("index 12 out of range 0x7ffa1000"),
            signature("index 999 out of range 0x7ffb2222")
        );
        assert_ne!(
            signature("index 1 out of range"),
            signature("length overflow")
        );
    }

    /// Shared with `tpt-testkit-cli`'s `crash-dedup`: both must produce this value.
    #[test]
    fn signature_matches_cli_test_vector() {
        assert_eq!(signature("index 1 out of range"), 0xd753_b54d_f346_4306);
    }

    #[test]
    fn dedup_groups_and_keeps_smallest() {
        let failures = vec![
            Failure {
                input: b"long input".to_vec(),
                message: "index 3 bad".to_owned(),
            },
            Failure {
                input: b"ab".to_vec(),
                message: "index 9 bad".to_owned(),
            },
            Failure {
                input: b"zz".to_vec(),
                message: "other".to_owned(),
            },
        ];
        let groups = dedup(&failures);
        assert_eq!(groups.len(), 2);
        let grouped = groups.iter().find(|g| g.message == "index 3 bad").unwrap();
        assert_eq!(grouped.count, 2);
        assert_eq!(grouped.smallest, b"ab");

        let known = [signature("other")];
        let fresh = unknown_groups(&groups, &known);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].message, "index 3 bad");
    }

    #[test]
    fn corpus_minimization_keeps_covering_inputs_only() {
        let inputs = vec![
            b"aaaa".to_vec(),
            b"a".to_vec(),
            b"b".to_vec(),
            b"ab".to_vec(),
            b"zz".to_vec(),
        ];
        // Feature = the distinct bytes of the input. Feature 'z' is only in "zz".
        let kept = minimize_corpus(&inputs, |data| data.iter().map(|&b| u64::from(b)).collect());
        assert!(kept.contains(&b"a".to_vec()));
        assert!(kept.contains(&b"b".to_vec()));
        assert!(kept.contains(&b"zz".to_vec()));
        assert!(!kept.contains(&b"aaaa".to_vec()));
        assert!(!kept.contains(&b"ab".to_vec()));
    }
}
