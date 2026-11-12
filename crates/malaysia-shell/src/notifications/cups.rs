use std::collections::HashMap;
use std::time::Duration;

use malaysia_services::cups::{
    BOOLEAN, GET_JOBS, Job, KEYWORD, NAME, URI, attribute, exchange, parse_jobs, request,
};

use super::live::{self, Activity};
use super::server::Link;

const POLL: Duration = Duration::from_secs(2);
const IDLE: Duration = Duration::from_secs(60);
const CANCEL_JOB: u16 = 0x0008;
const PROCESSING_STOPPED: i32 = 6;
const HELD: i32 = 4;
const WANTED: [&str; 6] = [
    "job-id",
    "job-name",
    "job-state",
    "job-media-sheets-completed",
    "job-impressions",
    "job-printer-uri",
];

fn user() -> &'static str {
    static USER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    USER.get_or_init(|| malaysia_services::accounts::login_name().unwrap_or_default())
}

fn get_jobs_request(user: &str) -> Vec<u8> {
    request(GET_JOBS, |buffer| {
        attribute(buffer, URI, "printer-uri", b"ipp://localhost/");
        attribute(buffer, NAME, "requesting-user-name", user.as_bytes());
        attribute(buffer, KEYWORD, "which-jobs", b"not-completed");
        attribute(buffer, BOOLEAN, "my-jobs", &[1]);
        for (index, wanted) in WANTED.iter().enumerate() {
            let name = if index == 0 {
                "requested-attributes"
            } else {
                ""
            };
            attribute(buffer, KEYWORD, name, wanted.as_bytes());
        }
    })
}

pub(crate) fn cancel(job: i32) {
    std::thread::spawn(move || {
        let body = request(CANCEL_JOB, |buffer| {
            attribute(
                buffer,
                URI,
                "job-uri",
                format!("ipp://localhost/jobs/{job}").as_bytes(),
            );
            attribute(buffer, NAME, "requesting-user-name", user().as_bytes());
        });
        if let Err(error) = exchange(&body) {
            tracing::warn!(%error, job, "could not cancel the print job");
        }
    });
}

pub(crate) fn start(link: Link) {
    std::thread::Builder::new()
        .name("malaysia-print-jobs".to_string())
        .spawn(move || {
            let mut known: HashMap<i32, (i64, bool)> = HashMap::new();
            loop {
                let jobs = exchange(&get_jobs_request(user()))
                    .ok()
                    .and_then(|reply| parse_jobs(&reply));
                let Some(jobs) = jobs else {
                    std::thread::sleep(IDLE);
                    continue;
                };
                let now = chrono::Utc::now().timestamp();
                for job in &jobs {
                    let (started, ended) = *known.entry(job.id).or_insert((now, false));
                    if !ended {
                        live::upsert(&link, activity(job, started));
                    }
                }
                known.retain(|id, (started, ended)| {
                    let present = jobs.iter().any(|job| job.id == *id);
                    if !*ended && (!present || now - *started > live::LONGEST) {
                        *ended = true;
                        live::end(&link, &format!("print-{id}"));
                    }
                    present
                });
                std::thread::sleep(POLL);
            }
        })
        .expect("malaysia-print-jobs thread");
}

fn activity(job: &Job, started: i64) -> Activity {
    let mut activity = Activity::new(format!("print-{}", job.id), live::PRINT, started);
    activity.title = job.name.clone();
    activity.detail = job.printer.clone();
    activity.unit = live::PAGES.into();
    activity.processed = job.done.max(0) as u64;
    activity.total = job.pages.max(0) as u64;
    activity.cancellable = true;
    activity.paused = matches!(job.state, HELD | PROCESSING_STOPPED);
    activity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_get_jobs_request_asks_for_this_users_unfinished_jobs() {
        let body = get_jobs_request("ana");
        assert_eq!(&body[..4], &[2, 0, 0x00, 0x0a]);
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("not-completed")
                && text.contains("ana")
                && text.contains("job-media-sheets-completed")
        );
        assert_eq!(body.last(), Some(&malaysia_services::cups::END));
    }

    #[test]
    fn a_job_reports_its_printed_fraction() {
        let job = Job {
            id: 12,
            done: 3,
            pages: 12,
            ..Job::default()
        };
        assert_eq!(activity(&job, 0).fraction(), Some(0.25));
    }
}
