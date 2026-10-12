//! In-process mutation engine: a deterministic, dependency-free fuzz loop that runs on stable.
//!
//! Inputs are mutated from a corpus. A mutant joins the corpus when the target reports a feature
//! the engine has not seen before. Features are whatever the target chooses to report, such as
//! error positions or branch IDs from manual instrumentation. A target that reports nothing
//! turns the engine into plain random mutation of the seeds.
//!
//! Sancov edge coverage needs nightly instrumentation and is not part of this module.
//!
//! The engine is seeded, so the same seed and target give the same run on every platform.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use super::{Failure, fnv1a, panic_message};

/// Upper bound on generated input size. Keeps growth from exhausting memory over long runs.
pub const MAX_INPUT_LEN: usize = 64 * 1024;

const INTERESTING_U32: [u32; 5] = [0, 1, u32::MAX, i32::MAX as u32, 0x8000_0000];

/// Deterministic xorshift64* generator. The same seed gives the same sequence everywhere.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// Creates a generator. A zero seed is replaced with a fixed non-zero constant.
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// Returns the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Returns a value in `0..bound`.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is zero.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be non-zero");
        (self.next_u64() % bound as u64) as usize
    }
}

/// Applies one to four random mutations to `input`.
///
/// `corpus` supplies material for splicing and `dictionary` supplies tokens to insert. Both may
/// be empty. The result is capped at [`MAX_INPUT_LEN`] bytes.
pub fn mutate(input: &[u8], corpus: &[Vec<u8>], dictionary: &[Vec<u8>], rng: &mut Rng) -> Vec<u8> {
    let mut out = input.to_vec();
    let rounds = 1 + rng.below(4);
    for _ in 0..rounds {
        apply_one(&mut out, corpus, dictionary, rng);
    }
    out.truncate(MAX_INPUT_LEN);
    out
}

fn apply_one(data: &mut Vec<u8>, corpus: &[Vec<u8>], dictionary: &[Vec<u8>], rng: &mut Rng) {
    let len = data.len();
    match rng.below(10) {
        0 if len > 0 => {
            let index = rng.below(len);
            let bit = rng.below(8);
            data[index] ^= 1 << bit;
        }
        1 if len > 0 => {
            let index = rng.below(len);
            data[index] = rng.next_u64() as u8;
        }
        2 => {
            let at = rng.below(len + 1);
            data.insert(at, rng.next_u64() as u8);
        }
        3 if len > 0 => {
            let start = rng.below(len);
            let count = 1 + rng.below((len - start).min(8));
            data.drain(start..start + count);
        }
        4 if len > 0 => {
            let start = rng.below(len);
            let count = 1 + rng.below((len - start).min(32));
            let chunk = data[start..start + count].to_vec();
            let at = rng.below(len + 1);
            insert_slice(data, at, &chunk);
        }
        5 if len > 0 => {
            let index = rng.below(len);
            let delta = 1 + rng.below(35) as u8;
            data[index] = if rng.below(2) == 0 {
                data[index].wrapping_add(delta)
            } else {
                data[index].wrapping_sub(delta)
            };
        }
        6 if !dictionary.is_empty() => {
            let token = &dictionary[rng.below(dictionary.len())];
            let at = rng.below(len + 1);
            insert_slice(data, at, token);
        }
        7 if !corpus.is_empty() => {
            let other = &corpus[rng.below(corpus.len())];
            if !other.is_empty() {
                let start = rng.below(other.len());
                let count = 1 + rng.below((other.len() - start).min(64));
                let at = rng.below(len + 1);
                insert_slice(data, at, &other[start..start + count]);
            }
        }
        8 if len >= 4 => {
            let index = rng.below(len - 3);
            let value = INTERESTING_U32[rng.below(INTERESTING_U32.len())];
            let bytes = if rng.below(2) == 0 {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            data[index..index + 4].copy_from_slice(&bytes);
        }
        9 => {
            let at = rng.below(len + 1);
            data.truncate(at);
        }
        _ => data.push(rng.next_u64() as u8),
    }
}

fn insert_slice(data: &mut Vec<u8>, at: usize, chunk: &[u8]) {
    let tail = data.split_off(at);
    data.extend_from_slice(chunk);
    data.extend(tail);
}

/// Result of a fuzzing run: the corpus so far and every distinct crashing input.
///
/// Crashes are recorded once per distinct input. Panic output is printed by the default panic
/// hook while the engine runs.
#[derive(Debug)]
pub struct Engine {
    rng: Rng,
    corpus: Vec<Vec<u8>>,
    features: BTreeSet<u64>,
    dictionary: Vec<Vec<u8>>,
    crashes: Vec<Failure>,
    crash_inputs: BTreeSet<Vec<u8>>,
    executions: u64,
    seeded: bool,
}

impl Engine {
    /// Creates an engine with the given RNG seed and starting corpus.
    /// # Examples
    /// ```
    /// use tpt_testkit::fuzz::Engine;
    ///
    /// let mut engine = Engine::new(7, vec![b"seed".to_vec()]);
    /// engine.run(100, |data| vec![data.len() as u64]);
    /// assert_eq!(engine.executions(), 101);
    /// assert!(engine.crashes().is_empty());
    /// ```
    pub fn new(seed: u64, seeds: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            rng: Rng::new(seed),
            corpus: seeds.into_iter().collect(),
            features: BTreeSet::new(),
            dictionary: Vec::new(),
            crashes: Vec::new(),
            crash_inputs: BTreeSet::new(),
            executions: 0,
            seeded: false,
        }
    }

    /// Sets the tokens that dictionary mutations insert.
    #[must_use]
    pub fn with_dictionary(mut self, tokens: &[&[u8]]) -> Self {
        self.dictionary = tokens.iter().map(|token| token.to_vec()).collect();
        self
    }

    /// Runs `iterations` mutate-and-execute steps.
    ///
    /// The first call also executes every seed, so seeds that report features are kept and seed
    /// crashes are recorded. `target` returns the features an input exercised. It may return an
    /// empty vector. A panic in `target` is recorded as a crash.
    pub fn run<F>(&mut self, iterations: u64, target: F)
    where
        F: Fn(&[u8]) -> Vec<u64>,
    {
        if self.corpus.is_empty() {
            self.corpus.push(Vec::new());
        }
        if !self.seeded {
            self.seeded = true;
            for seed in self.corpus.clone() {
                self.execute(seed, &target);
            }
        }
        for _ in 0..iterations {
            let parent = self.rng.below(self.corpus.len());
            let child = mutate(
                &self.corpus[parent],
                &self.corpus,
                &self.dictionary,
                &mut self.rng,
            );
            if self.execute(child.clone(), &target) {
                self.corpus.push(child);
            }
        }
    }

    /// Current corpus, including seeds and kept mutants.
    pub fn corpus(&self) -> &[Vec<u8>] {
        &self.corpus
    }

    /// Distinct inputs that made the target panic, in discovery order.
    pub fn crashes(&self) -> &[Failure] {
        &self.crashes
    }

    /// Number of target executions so far, including seeds.
    pub fn executions(&self) -> u64 {
        self.executions
    }

    /// Number of distinct features reported so far.
    pub fn features_seen(&self) -> usize {
        self.features.len()
    }

    /// Runs one input. Returns true if it reported a feature not seen before.
    fn execute<F>(&mut self, input: Vec<u8>, target: &F) -> bool
    where
        F: Fn(&[u8]) -> Vec<u64>,
    {
        self.executions += 1;
        match panic::catch_unwind(AssertUnwindSafe(|| target(&input))) {
            Ok(features) => {
                let mut new = false;
                for feature in features {
                    new |= self.features.insert(feature);
                }
                new
            }
            Err(payload) => {
                if self.crash_inputs.insert(input.clone()) {
                    self.crashes.push(Failure {
                        message: panic_message(payload.as_ref()),
                        input,
                    });
                }
                false
            }
        }
    }
}

/// Reads every regular file in `dir`, in name order.
///
/// # Errors
///
/// Returns any I/O error from reading the directory or its files.
pub fn read_corpus(dir: impl AsRef<Path>) -> io::Result<Vec<Vec<u8>>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    paths.retain(|path| path.is_file());
    paths.sort();
    paths.iter().map(fs::read).collect()
}

/// Writes each distinct input to `dir` as `<fnv1a-hex>`, creating `dir` if needed.
///
/// Names depend only on content, so saving an unchanged corpus rewrites identical files and
/// adds nothing new. Returns the paths written.
///
/// # Errors
///
/// Returns any I/O error from creating `dir` or writing a file.
pub fn save_corpus(corpus: &[Vec<u8>], dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    let unique: BTreeSet<&Vec<u8>> = corpus.iter().collect();
    unique
        .into_iter()
        .map(|input| {
            let path = dir.join(format!("{:016x}", fnv1a(input)));
            fs::write(&path, input)?;
            Ok(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            let value = a.next_u64();
            assert_eq!(value, b.next_u64());
        }
        for _ in 0..100 {
            assert!(a.below(7) < 7);
        }
    }

    #[test]
    fn zero_seed_is_usable() {
        let mut rng = Rng::new(0);
        assert_ne!(rng.next_u64(), 0);
    }

    #[test]
    fn mutants_are_capped_and_can_grow_from_empty() {
        let mut rng = Rng::new(7);
        let grown = (0..200).any(|_| !mutate(&[], &[], &[], &mut rng).is_empty());
        assert!(grown);
        let big = vec![0_u8; MAX_INPUT_LEN];
        assert!(mutate(&big, &[], &[], &mut rng).len() <= MAX_INPUT_LEN);
    }

    #[test]
    fn finds_a_planted_crash_using_the_dictionary() {
        let mut engine = Engine::new(1, vec![b"xxxxxxxx".to_vec()]).with_dictionary(&[b"FUZZ"]);
        engine.run(20_000, |data| {
            if contains(data, b"FUZZ") {
                panic!("planted");
            }
            Vec::new()
        });
        assert!(!engine.crashes().is_empty());
        assert_eq!(engine.crashes()[0].message, "planted");
        assert!(contains(&engine.crashes()[0].input, b"FUZZ"));
    }

    #[test]
    fn feedback_grows_the_corpus() {
        let mut engine = Engine::new(3, vec![b"abc".to_vec()]);
        engine.run(500, |data| vec![data.len() as u64]);
        assert!(engine.corpus().len() > 1);
        assert!(engine.features_seen() > 1);
        assert_eq!(engine.executions(), 501);
    }

    #[test]
    fn runs_are_reproducible_for_a_seed() {
        let target = |data: &[u8]| {
            vec![
                data.len() as u64,
                u64::from(data.first().copied().unwrap_or(0)),
            ]
        };
        let mut a = Engine::new(9, vec![b"seed".to_vec()]);
        let mut b = Engine::new(9, vec![b"seed".to_vec()]);
        a.run(300, target);
        b.run(300, target);
        assert_eq!(a.corpus(), b.corpus());
    }

    #[test]
    fn corpus_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("tpt-corpus-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let corpus = vec![b"one".to_vec(), b"two".to_vec(), b"one".to_vec()];
        let written = save_corpus(&corpus, &dir).unwrap();
        assert_eq!(written.len(), 2, "duplicates are stored once");
        let mut loaded = read_corpus(&dir).unwrap();
        loaded.sort();
        assert_eq!(loaded, vec![b"one".to_vec(), b"two".to_vec()]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
