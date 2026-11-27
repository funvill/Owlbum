//! Model downloads (docs/07-tagging.md, "Model download"). Files come only
//! from the URL the catalogue pins, over HTTPS, from Hugging Face hosts;
//! redirects are followed by hand and every hop is checked. Each file is
//! checked against the SHA-256 compiled into Owlbum before it is used.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use super::catalogue::{ModelChoice, ModelFile};
use super::hosts;
use super::install;
use crate::text::tagging as words;

/// The most redirect hops followed.
pub const MAX_REDIRECTS: usize = 5;
/// No byte for this long fails the download.
pub const STALL: Duration = Duration::from_secs(60);

/// An answer to a GET.
pub struct Response {
    pub status: u16,
    /// Lower-case names.
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Read + Send>,
}

impl Response {
    /// A header's value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Why a GET got no answer.
#[derive(Debug)]
pub enum FetchError {
    /// No connection, a DNS failure, a TLS failure.
    Unreachable(String),
}

/// Performs one GET, following no redirect, sending no cookie and no
/// credential.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response, FetchError>;
}

/// The real client.
pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(format!("Owlbum/{}", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(STALL))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Default for HttpFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetch for HttpFetch {
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response, FetchError> {
        let mut req = self.agent.get(url);
        if let Some(from) = range_from {
            req = req.header("Range", &format!("bytes={from}-"));
        }
        let res = req
            .call()
            .map_err(|e| FetchError::Unreachable(e.to_string()))?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .filter_map(|(n, v)| Some((n.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_owned())))
            .collect();
        Ok(Response {
            status,
            headers,
            body: Box::new(res.into_body().into_reader()),
        })
    }
}

/// How a download ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    /// The reason, for "Couldn't download {label}: {reason}".
    Failed(String),
}

/// One download of one model.
pub struct Job<'a> {
    pub model: &'a ModelChoice,
    /// `<models>/<id>`.
    pub dir: &'a Path,
    pub fetch: &'a dyn Fetch,
    pub cancel: &'a AtomicBool,
    /// Called with `(received, total)` as bytes arrive. `received` can go
    /// down when a server ignores a range.
    pub progress: &'a mut dyn FnMut(u64, u64),
    /// No byte for this long fails the download ([`STALL`]).
    pub stall: Duration,
}

enum Stop {
    Cancelled,
    Failed(String),
}

impl From<Stop> for Outcome {
    fn from(s: Stop) -> Self {
        match s {
            Stop::Cancelled => Outcome::Cancelled,
            Stop::Failed(r) => Outcome::Failed(r),
        }
    }
}

/// Downloads every file of the model that is not already there and sound.
pub fn download(job: &mut Job<'_>) -> Outcome {
    let total = job.model.bytes();
    let mut received = 0u64;
    (job.progress)(received, total);
    for f in job.model.files {
        if job.cancel.load(Ordering::Relaxed) {
            return Outcome::Cancelled;
        }
        let target = install::file_path(job.dir, f);
        if std::fs::metadata(&target).is_ok_and(|m| m.len() == f.bytes) {
            let marker = install::read_marker(job.dir);
            let vouched = marker.revision == job.model.revision
                && marker.files.get(f.path).is_some_and(|e| {
                    e.sha256 == f.sha256
                        && std::fs::metadata(&target).is_ok_and(|m| m.len() == e.bytes)
                });
            if vouched || install::sha256_file(&target).is_ok_and(|h| h == f.sha256) {
                let _ = install::vouch(job.dir, job.model, f);
                received += f.bytes;
                (job.progress)(received, total);
                continue;
            }
        }
        let _ = std::fs::remove_file(&target);
        if let Err(stop) = download_file(job, f, &mut received, total) {
            return stop.into();
        }
    }
    Outcome::Done
}

fn write_error(path: &str, e: &std::io::Error) -> Stop {
    let full = e.kind() == std::io::ErrorKind::StorageFull
        || matches!(e.raw_os_error(), Some(28 | 39 | 112));
    Stop::Failed(if full {
        words::disk_full(path)
    } else {
        words::write_failed(path, &e.to_string())
    })
}

/// GETs `url`, following up to [`MAX_REDIRECTS`] redirects by hand. Every
/// hop must pass the allowed-hosts rule; the range goes on every hop.
fn get(fetch: &dyn Fetch, url: &str, range_from: Option<u64>) -> Result<Response, Stop> {
    let mut url = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        if let Err(host) = hosts::allowed(&url) {
            return Err(Stop::Failed(words::host_not_allowed(&host)));
        }
        let res = fetch
            .get(&url, range_from)
            .map_err(|_| Stop::Failed(words::UNREACHABLE.to_owned()))?;
        if matches!(res.status, 301 | 302 | 303 | 307 | 308) {
            let Some(location) = res.header("location") else {
                return Err(Stop::Failed(words::REDIRECT_WITHOUT_LOCATION.to_owned()));
            };
            url = hosts::resolve(&url, location);
            continue;
        }
        return Ok(res);
    }
    Err(Stop::Failed(words::TOO_MANY_REDIRECTS.to_owned()))
}

fn download_file(job: &mut Job<'_>, f: &ModelFile, received: &mut u64, total: u64) -> Result<(), Stop> {
    let target = install::file_path(job.dir, f);
    let part = install::part_path(job.dir, f);
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent).map_err(|e| write_error(f.path, &e))?;
    }
    let mut have = std::fs::metadata(&part).map_or(0, |m| m.len());
    if have > f.bytes {
        let _ = std::fs::remove_file(&part);
        have = 0;
    }
    *received += have;
    (job.progress)(*received, total);
    if have < f.bytes {
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            job.model.repo, job.model.revision, f.path
        );
        let mut res = get(job.fetch, &url, (have > 0).then_some(have))?;
        if have > 0 && matches!(res.status, 200 | 416) {
            // The server ignored or refused the range: start again.
            let _ = std::fs::remove_file(&part);
            *received -= have;
            have = 0;
            (job.progress)(*received, total);
            if res.status == 416 {
                res = get(job.fetch, &url, None)?;
            }
        }
        if !matches!(res.status, 200 | 206) {
            return Err(Stop::Failed(words::bad_status(res.status, f.path)));
        }
        if res.status == 206 {
            let starts = res
                .header("content-range")
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if starts != Some(have) {
                let _ = std::fs::remove_file(&part);
                return Err(Stop::Failed(words::bad_status(res.status, f.path)));
            }
        }
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(have > 0)
            .write(true)
            .truncate(have == 0)
            .open(&part)
            .map_err(|e| write_error(f.path, &e))?;
        copy_body(job, f, res.body, &mut out, have, received, total)?;
        out.sync_all().map_err(|e| write_error(f.path, &e))?;
    }
    let size = std::fs::metadata(&part).map_or(0, |m| m.len());
    if size != f.bytes || !install::sha256_file(&part).is_ok_and(|h| h == f.sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(Stop::Failed(words::failed_check(f.path)));
    }
    std::fs::rename(&part, &target).map_err(|e| write_error(f.path, &e))?;
    install::vouch(job.dir, job.model, f).map_err(|e| write_error(f.path, &e))?;
    Ok(())
}

/// Copies the body to the `.part` file. The body is read on a thread of
/// its own, so that a stalled connection and a cancel are noticed while a
/// read is blocked.
fn copy_body(
    job: &mut Job<'_>,
    f: &ModelFile,
    mut body: Box<dyn Read + Send>,
    out: &mut std::fs::File,
    mut have: u64,
    received: &mut u64,
    total: u64,
) -> Result<(), Stop> {
    let (tx, rx) = mpsc::sync_channel::<std::io::Result<Vec<u8>>>(4);
    std::thread::Builder::new()
        .name("owlbum-download-body".into())
        .spawn(move || {
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                match body.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(Ok(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        break;
                    }
                }
            }
        })
        .map_err(|e| Stop::Failed(e.to_string()))?;
    let slice = Duration::from_millis(100).min(job.stall);
    let mut quiet = Duration::ZERO;
    loop {
        if job.cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        match rx.recv_timeout(slice) {
            Ok(Ok(chunk)) => {
                quiet = Duration::ZERO;
                out.write_all(&chunk).map_err(|e| write_error(f.path, &e))?;
                have += chunk.len() as u64;
                *received += chunk.len() as u64;
                (job.progress)(*received, total);
                if have > f.bytes {
                    // Too many bytes: the check after fails it.
                    return Ok(());
                }
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(Stop::Failed(words::STALLED.to_owned()));
            }
            // A connection cut short: the size check after fails it.
            Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                quiet += slice;
                if quiet >= job.stall {
                    return Err(Stop::Failed(words::STALLED.to_owned()));
                }
            }
        }
    }
}

/// A fake server for tests: serves the fake models' files from memory.
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// What the fake server does wrong.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub enum Fault {
        #[default]
        None,
        /// Answers 200 with the whole file to a range request.
        IgnoreRange,
        /// Sends only half of each body, then ends.
        CutShort,
        /// Flips a byte of each body.
        Corrupt,
        /// Sends half of each body, then nothing more.
        Stall,
        /// Redirects to a host that is not allowed.
        BadRedirect,
        /// Redirects without a location.
        NoLocation,
        /// Redirects forever.
        RedirectLoop,
        /// Answers 404.
        NotFound,
        /// Cannot be reached.
        Offline,
    }

    /// Serves files by URL path. The first hop on `huggingface.co` redirects
    /// to `cdn-lfs.hf.co`, which serves.
    #[derive(Default)]
    pub struct FakeFetch {
        pub files: Mutex<std::collections::HashMap<String, Vec<u8>>>,
        pub fault: Mutex<Fault>,
        /// Every request: URL and range.
        pub log: Mutex<Vec<(String, Option<u64>)>>,
        /// Called once per body chunk sent; tests use it to cancel.
        pub on_chunk: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    }

    impl FakeFetch {
        /// Serves every file of every model in `models`.
        pub fn serving(models: &[ModelChoice]) -> Self {
            let f = Self::default();
            for m in models {
                for file in m.files {
                    let key = format!("/{}/resolve/{}/{}", m.repo, m.revision, file.path);
                    f.files
                        .lock()
                        .unwrap()
                        .insert(key, crate::tagging::fake::file_content(m.id, file.path));
                }
            }
            f
        }

        pub fn set_fault(&self, fault: Fault) {
            *self.fault.lock().unwrap() = fault;
        }

        pub fn requests(&self) -> Vec<(String, Option<u64>)> {
            self.log.lock().unwrap().clone()
        }
    }

    struct Chunks {
        data: Vec<u8>,
        at: usize,
        stall: bool,
    }

    impl Read for Chunks {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.at >= self.data.len() {
                if self.stall {
                    std::thread::sleep(Duration::from_millis(300));
                    return Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
                }
                return Ok(0);
            }
            let n = buf.len().min(7).min(self.data.len() - self.at);
            buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
            self.at += n;
            Ok(n)
        }
    }

    impl Fetch for FakeFetch {
        fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response, FetchError> {
            self.log.lock().unwrap().push((url.to_owned(), range_from));
            let fault = *self.fault.lock().unwrap();
            if fault == Fault::Offline {
                return Err(FetchError::Unreachable("offline".into()));
            }
            let redirect = |to: Option<&str>| Response {
                status: 302,
                headers: to.map(|t| vec![("location".to_owned(), t.to_owned())]).unwrap_or_default(),
                body: Box::new(std::io::empty()),
            };
            let (host, path) = url
                .strip_prefix("https://")
                .and_then(|r| r.split_once('/'))
                .map(|(h, p)| (h.to_owned(), format!("/{p}")))
                .unwrap_or_default();
            if host == "huggingface.co" {
                return Ok(match fault {
                    Fault::BadRedirect => redirect(Some("https://evil.example.com/x")),
                    Fault::NoLocation => redirect(None),
                    _ => redirect(Some(&format!("https://cdn-lfs.hf.co{path}"))),
                });
            }
            if fault == Fault::RedirectLoop {
                return Ok(redirect(Some(&format!("https://cdn-lfs.hf.co{path}"))));
            }
            let Some(mut data) = self.files.lock().unwrap().get(&path).cloned() else {
                return Ok(Response {
                    status: 404,
                    headers: Vec::new(),
                    body: Box::new(std::io::empty()),
                });
            };
            if fault == Fault::NotFound {
                return Ok(Response {
                    status: 404,
                    headers: Vec::new(),
                    body: Box::new(std::io::empty()),
                });
            }
            if fault == Fault::Corrupt {
                data[0] ^= 1;
            }
            let len = data.len() as u64;
            let (status, headers, body) = match range_from {
                Some(from) if fault != Fault::IgnoreRange => {
                    if from >= len {
                        (416, Vec::new(), Vec::new())
                    } else {
                        let cr = format!("bytes {from}-{}/{len}", len - 1);
                        (206, vec![("content-range".to_owned(), cr)], data[from as usize..].to_vec())
                    }
                }
                _ => (200, Vec::new(), data),
            };
            let body = match fault {
                Fault::CutShort | Fault::Stall => body[..body.len() / 2].to_vec(),
                _ => body,
            };
            if let Some(cb) = self.on_chunk.lock().unwrap().as_ref() {
                cb();
            }
            Ok(Response {
                status,
                headers,
                body: Box::new(Chunks {
                    data: body,
                    at: 0,
                    stall: fault == Fault::Stall,
                }),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeFetch, Fault};
    use super::*;
    use crate::tagging::fake::FAKE_MODELS;
    use crate::tagging::install::{InstallState, quick_state};

    struct Run {
        tmp: tempfile::TempDir,
        fetch: FakeFetch,
        cancel: AtomicBool,
    }

    fn setup() -> Run {
        Run {
            tmp: tempfile::tempdir().unwrap(),
            fetch: FakeFetch::serving(FAKE_MODELS),
            cancel: AtomicBool::new(false),
        }
    }

    fn model() -> &'static ModelChoice {
        &FAKE_MODELS[2]
    }

    impl Run {
        fn dir(&self) -> std::path::PathBuf {
            self.tmp.path().join(model().id)
        }

        fn go(&self) -> (Outcome, Vec<(u64, u64)>) {
            let mut seen = Vec::new();
            let mut progress = |r, t| seen.push((r, t));
            let dir = self.dir();
            let outcome = download(&mut Job {
                model: model(),
                dir: &dir,
                fetch: &self.fetch,
                cancel: &self.cancel,
                progress: &mut progress,
                stall: Duration::from_millis(200),
            });
            (outcome, seen)
        }
    }

    #[test]
    fn a_clean_download_installs_the_model() {
        let r = setup();
        let (outcome, seen) = r.go();
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(quick_state(&r.dir(), model()), InstallState::Ok);
        assert_eq!(seen.last(), Some(&(model().bytes(), model().bytes())));
        // One redirect hop for each file, to an allowed host.
        let log = r.fetch.requests();
        assert_eq!(log.len(), 4);
        assert!(log[1].0.starts_with("https://cdn-lfs.hf.co/"));
        // A second download hashes the files and fetches nothing.
        let (outcome, _) = r.go();
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(r.fetch.requests().len(), 4);
    }

    #[test]
    fn a_part_file_is_resumed_with_a_range() {
        let r = setup();
        let f = &model().files[0];
        let part = install::part_path(&r.dir(), f);
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        let content = crate::tagging::fake::file_content(model().id, f.path);
        std::fs::write(&part, &content[..10]).unwrap();
        assert_eq!(r.go().0, Outcome::Done);
        let log = r.fetch.requests();
        assert_eq!(log[0].1, Some(10));
        assert_eq!(log[1].1, Some(10), "the range is sent on every hop");
        assert_eq!(quick_state(&r.dir(), model()), InstallState::Ok);
    }

    #[test]
    fn a_server_that_ignores_the_range_starts_the_file_again() {
        let r = setup();
        r.fetch.set_fault(Fault::IgnoreRange);
        let f = &model().files[0];
        let part = install::part_path(&r.dir(), f);
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, b"0123456789").unwrap();
        let (outcome, seen) = r.go();
        assert_eq!(outcome, Outcome::Done);
        // `received` went down when the part was thrown away.
        assert!(seen.windows(2).any(|w| w[1].0 < w[0].0));
        assert_eq!(quick_state(&r.dir(), model()), InstallState::Ok);
    }

    #[test]
    fn a_wrong_hash_deletes_the_part() {
        let r = setup();
        r.fetch.set_fault(Fault::Corrupt);
        let (outcome, _) = r.go();
        assert_eq!(
            outcome,
            Outcome::Failed(
                "model.bin did not pass its check (wrong size or SHA-256); it was deleted. Try again."
                    .into()
            )
        );
        assert!(!install::part_path(&r.dir(), &model().files[0]).exists());
    }

    #[test]
    fn a_wrong_file_already_present_is_fetched_again() {
        let r = setup();
        let f = &model().files[0];
        std::fs::create_dir_all(r.dir()).unwrap();
        let mut wrong = crate::tagging::fake::file_content(model().id, f.path);
        wrong[3] ^= 1;
        std::fs::write(install::file_path(&r.dir(), f), wrong).unwrap();
        assert_eq!(r.go().0, Outcome::Done);
        assert_eq!(quick_state(&r.dir(), model()), InstallState::Ok);
        assert_eq!(r.fetch.requests().len(), 4);
    }

    #[test]
    fn cancel_keeps_the_part_and_is_not_an_error() {
        let r = setup();
        r.cancel.store(true, Ordering::Relaxed);
        assert_eq!(r.go().0, Outcome::Cancelled);
        assert!(r.fetch.requests().is_empty());
    }

    #[test]
    fn every_row_of_the_wording_table() {
        let cases = [
            (Fault::NotFound, "Hugging Face answered 404 for model.bin.".to_owned()),
            (Fault::BadRedirect, "Download host not allowed: evil.example.com".to_owned()),
            (Fault::NoLocation, "Hugging Face sent a redirect without a location.".to_owned()),
            (Fault::RedirectLoop, "Too many redirects from Hugging Face.".to_owned()),
            (
                Fault::Offline,
                "Couldn't reach Hugging Face. Check the internet connection and try again.".to_owned(),
            ),
            (Fault::Stall, "The download stalled. Try again.".to_owned()),
            (
                Fault::CutShort,
                "model.bin did not pass its check (wrong size or SHA-256); it was deleted. Try again."
                    .to_owned(),
            ),
        ];
        for (fault, reason) in cases {
            let r = setup();
            r.fetch.set_fault(fault);
            assert_eq!(r.go().0, Outcome::Failed(reason), "{fault:?}");
        }
        assert_eq!(
            write_error("a.onnx", &std::io::Error::from(std::io::ErrorKind::StorageFull)).msg(),
            "Not enough disk space for a.onnx. Free some space and download again."
        );
        assert_eq!(
            write_error("a.onnx", &std::io::Error::other("denied")).msg(),
            "Couldn't write a.onnx: denied"
        );
    }

    impl Stop {
        fn msg(self) -> String {
            match self {
                Stop::Failed(m) => m,
                Stop::Cancelled => String::new(),
            }
        }
    }

    #[test]
    fn a_stalled_part_is_kept_for_the_next_try() {
        let r = setup();
        r.fetch.set_fault(Fault::Stall);
        r.go();
        assert!(install::part_path(&r.dir(), &model().files[0]).exists());
        r.fetch.set_fault(Fault::None);
        assert_eq!(r.go().0, Outcome::Done);
    }
}
