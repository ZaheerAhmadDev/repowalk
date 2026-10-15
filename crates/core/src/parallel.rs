//! Deterministic parallel work over per-file jobs (F10).
//!
//! **std-only.** Built on [`std::thread::scope`] — no `rayon`, no `tokio`, no
//! thread-pool crate (the dependency budget forbids them). Every worker
//! produces an independent per-file result and the merge sorts by
//! [`FileJob::path`] (ties broken by input position), so the output is
//! byte-identical for any worker count and across repeated runs. A "sequential"
//! run is simply `workers = 1`.
//!
//! OWNER: INTEL-5 (this file is created by that card).
//!
//! # Using it from the learn pipeline
//!
//! The closure may capture shared read-only context (`&root`, a
//! `symbols_by_file` map, …) by reference; it must be `Sync`, and the captured
//! environment must outlive the call. Returning a *partial* [`Analysis`] per
//! file and merging the parts in path order is the intended integration:
//!
//! ```
//! use repowalk_core::parallel::{map_files, FileJob};
//!
//! let jobs = vec![FileJob::new("b.rs"), FileJob::new("a.rs")];
//! // `0` means "min(items, available_parallelism)"; output is path-sorted.
//! let upper = map_files(&jobs, 0, |job| job.path.to_uppercase());
//! assert_eq!(upper, vec!["A.RS".to_string(), "B.RS".to_string()]);
//! ```
use std::thread;

/// One unit of parallel work: a repo-relative file path.
///
/// The path is the deterministic merge key, so two jobs for the same path keep
/// their relative input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileJob {
    /// Repo-relative path (the sort key for the merge).
    pub path: String,
}

impl FileJob {
    /// A job for `path` (anything convertible into a `String`).
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

/// How many workers [`map_files`] will actually use.
///
/// `workers == 0` (the "default") resolves to
/// `min(items, available_parallelism())`; an explicit count is clamped to
/// `1..=max(items, 1)` so a tiny input never spawns idle threads and an empty
/// input never risks a zero divisor downstream.
pub fn effective_workers(items: usize, workers: usize) -> usize {
    let requested = if workers == 0 {
        thread::available_parallelism().map_or(1, |n| n.get())
    } else {
        workers
    };
    requested.clamp(1, items.max(1))
}

/// Map `f` over `items` in parallel, returning results sorted by
/// [`FileJob::path`] (ties by input position).
///
/// `workers` follows [`effective_workers`]. Each worker owns a contiguous
/// chunk and computes nothing but per-file results; the merge then sorts, so
/// the output is identical to a sequential run (`workers = 1`) and across
/// repeated runs, regardless of the order `items` arrived in or how the OS
/// schedules the threads.
///
/// A panic inside `f` propagates to the caller (the scoped threads are still
/// joined before unwinding).
pub fn map_files<T, F>(items: &[FileJob], workers: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(&FileJob) -> T + Sync,
{
    let n = effective_workers(items.len(), workers);

    let mut indexed: Vec<(usize, T)> = if n <= 1 || items.len() <= 1 {
        items
            .iter()
            .enumerate()
            .map(|(i, job)| (i, f(job)))
            .collect()
    } else {
        let chunk = items.len().div_ceil(n);
        thread::scope(|scope| {
            let handles: Vec<_> = items
                .chunks(chunk)
                .enumerate()
                .map(|(ci, slice)| {
                    let start = ci * chunk;
                    let f = &f;
                    scope.spawn(move || {
                        slice
                            .iter()
                            .enumerate()
                            .map(|(k, job)| (start + k, f(job)))
                            .collect::<Vec<(usize, T)>>()
                    })
                })
                .collect();

            let mut out: Vec<(usize, T)> = Vec::with_capacity(items.len());
            for handle in handles {
                out.extend(handle.join().expect("parallel worker panicked"));
            }
            out
        })
    };

    // Canonical order: path ascending, then original position.
    indexed.sort_by(|(ia, _), (ib, _)| {
        items[*ia]
            .path
            .cmp(&items[*ib].path)
            .then_with(|| ia.cmp(ib))
    });
    indexed.into_iter().map(|(_, value)| value).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jobs(paths: &[&str]) -> Vec<FileJob> {
        paths.iter().map(|p| FileJob::new(*p)).collect()
    }

    /// Serialise results into a single byte-ish string so a run can be compared
    /// byte-for-byte.
    fn fingerprint(values: &[String]) -> String {
        values.join("\u{1f}")
    }

    #[test]
    fn output_is_path_sorted_even_when_input_is_not() {
        let js = jobs(&["src/z.rs", "src/a.rs", "src/m.rs"]);
        let out = map_files(&js, 0, |job| format!("{}!", job.path));
        assert_eq!(
            out,
            vec!["src/a.rs!", "src/m.rs!", "src/z.rs!"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn two_runs_and_every_worker_count_are_byte_identical() {
        // Deliberately unsorted and > any plausible CPU count.
        let js: Vec<FileJob> = (0..257)
            .map(|i| FileJob::new(format!("src/mod{:03}/f{}.rs", i % 7, i)))
            .collect();
        let work = |job: &FileJob| format!("{}:{}", job.path, job.path.len());

        let sequential = map_files(&js, 1, work);
        let two = map_files(&js, 2, work);
        let many = map_files(&js, 8, work);
        let default = map_files(&js, 0, work);

        let fp = fingerprint(&sequential);
        assert_eq!(fingerprint(&two), fp, "workers=2 differs from sequential");
        assert_eq!(fingerprint(&many), fp, "workers=8 differs from sequential");
        assert_eq!(fingerprint(&default), fp, "default differs from sequential");

        // A second run of the same call is identical too.
        assert_eq!(fingerprint(&map_files(&js, 0, work)), fp);
        assert_eq!(sequential.len(), js.len());
    }

    #[test]
    fn empty_and_single_item_are_total() {
        let empty: Vec<FileJob> = Vec::new();
        let out: Vec<String> = map_files(&empty, 0, |job| job.path.clone());
        assert!(out.is_empty());
        assert_eq!(effective_workers(0, 0), 1);

        let one = jobs(&["only.rs"]);
        let out = map_files(&one, 4, |job| job.path.to_uppercase());
        assert_eq!(out, vec!["ONLY.RS".to_string()]);
        assert_eq!(effective_workers(1, 64), 1);
    }

    #[test]
    fn every_normal_job_returns_exactly_one_result() {
        let js = jobs(&["a.rs", "b.rs", "c.rs", "d.rs"]);
        let out = map_files(&js, 4, |job| job.path.len());
        assert_eq!(out.len(), 4);
        assert_eq!(out, vec![4, 4, 4, 4]);
    }

    #[test]
    fn workers_are_clamped_to_the_item_count() {
        assert_eq!(effective_workers(3, 99), 3, "never more workers than items");
        assert_eq!(effective_workers(0, 99), 1, "empty input still yields 1");
        let cpu = thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(effective_workers(5, 0), 5.min(cpu));
        assert_eq!(effective_workers(usize::MAX, 0), cpu);
    }

    #[test]
    fn duplicate_paths_keep_input_order_as_a_tiebreak() {
        let js = jobs(&["same.rs", "same.rs", "other.rs"]);
        let out = map_files(&js, 2, |job| job.path.clone());
        assert_eq!(out, vec!["other.rs", "same.rs", "same.rs"]);
    }

    /// Real per-file `analyze` timing on this repo: sequential vs parallel.
    ///
    /// Ignored by default (it walks and parses the checkout). Run with
    /// `cargo test -p repowalk-core parallel::tests::bench_ -- --ignored --nocapture`.
    /// Falls back to a synthetic CPU workload when the checkout is unavailable.
    #[test]
    #[ignore = "timing benchmark; run with --ignored --nocapture"]
    fn bench_parallel_analyze_on_this_repo() {
        use crate::analyze::Lang;
        use std::collections::BTreeMap;
        use std::path::Path;
        use std::time::Instant;

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent);

        let mut jobs: Vec<FileJob> = Vec::new();
        let mut langs: BTreeMap<String, Lang> = BTreeMap::new();
        let mut symbols_by_file: BTreeMap<String, Vec<crate::model::Symbol>> = BTreeMap::new();
        let mut symbols = 0usize;

        if let Some(root) = root {
            if let Ok(files) = crate::walk::walk(root) {
                for f in &files {
                    let Some(lang) = Lang::from_walk(&f.lang) else {
                        continue;
                    };
                    let Ok(text) = std::fs::read_to_string(root.join(&f.path)) else {
                        continue;
                    };
                    let (syms, _sites, _imports) = match f.lang.as_str() {
                        "rust" => crate::parse_rust::extract(&text, &f.path),
                        "python" => crate::parse_py::extract(&text, &f.path),
                        _ => (Vec::new(), Vec::new(), Vec::new()),
                    };
                    symbols += syms.len();
                    symbols_by_file.insert(f.path.clone(), syms);
                    langs.insert(f.path.clone(), lang);
                    jobs.push(FileJob::new(&f.path));
                }
            }
        }

        // Synthetic fallback: CPU-bound per-file work, same shape.
        let synthetic = jobs.is_empty();
        if synthetic {
            jobs = (0..800)
                .map(|i| FileJob::new(format!("synthetic/f{i}.rs")))
                .collect();
        }

        let work = |job: &FileJob| -> usize {
            if synthetic {
                let mut h = 0u64;
                for _ in 0..5000 {
                    for b in job.path.as_bytes() {
                        h = h.wrapping_mul(31).wrapping_add(u64::from(*b));
                    }
                }
                std::hint::black_box(h) as usize
            } else {
                let mut out = crate::analysis::Analysis::new();
                if let (Some(lang), Some(root)) = (langs.get(&job.path), root) {
                    let source = std::fs::read_to_string(root.join(&job.path)).unwrap_or_default();
                    let syms = symbols_by_file
                        .get(&job.path)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    crate::analyze::analyze(&job.path, *lang, &source, syms, &mut out);
                }
                out.variables.len() + out.cfgs.len() + out.types.len() + out.uses.len()
            }
        };

        let t0 = Instant::now();
        let seq = map_files(&jobs, 1, work);
        let seq_time = t0.elapsed();

        let t1 = Instant::now();
        let par = map_files(&jobs, 0, work);
        let par_time = t1.elapsed();

        assert_eq!(seq, par, "parallel result must equal sequential");
        let workers = effective_workers(jobs.len(), 0);
        let speedup = seq_time.as_secs_f64() / par_time.as_secs_f64().max(f64::EPSILON);
        println!(
            "bench(synthetic={synthetic}): files={} symbols={symbols} workers={workers} \
             seq={seq_time:?} par={par_time:?} speedup={speedup:.2}x",
            jobs.len()
        );
    }
}
