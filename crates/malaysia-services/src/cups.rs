use std::convert::Infallible;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use zbus::MatchRule;
use zbus::blocking::Connection;
use zbus::message::Type;

use crate::bus::Service;

const NOTIFIER: &str = "org.cups.cupsd.Notifier";
const SOCKET: &str = "/run/cups/cups.sock";
const TIMEOUT: Duration = Duration::from_secs(2);
// RFC 8011 §4.2.6 Get-Jobs; RFC 8010 §3.5 delimiter and value tags.
const GET_JOBS: u16 = 0x000a;
const OPERATION_ATTRIBUTES: u8 = 0x01;
const END_OF_ATTRIBUTES: u8 = 0x03;
const URI: u8 = 0x45;
const KEYWORD: u8 = 0x44;
const CHARSET: u8 = 0x47;
const NATURAL_LANGUAGE: u8 = 0x48;
const DELIMITERS: u8 = 0x10;
const SUCCESS_LIMIT: u16 = 0x0100;

#[derive(Default)]
pub struct Cups;

impl Service for Cups {
    const NAME: &'static str = NOTIFIER;
    type Snapshot = usize;
    type Command = Infallible;

    fn read(&mut self, _bus: &Connection) -> zbus::Result<usize> {
        queued_jobs().map_err(|error| zbus::Error::Failure(error.to_string()))
    }

    fn run(&mut self, _bus: &Connection, command: Infallible) -> zbus::Result<()> {
        match command {}
    }

    // cupsd's D-Bus notifier owns no bus name: its signals match by interface only.
    fn signals() -> zbus::Result<MatchRule<'static>> {
        Ok(MatchRule::builder()
            .msg_type(Type::Signal)
            .interface(NOTIFIER)?
            .build())
    }
}

fn queued_jobs() -> std::io::Result<usize> {
    let body = get_jobs_request();
    let mut stream = UnixStream::connect(SOCKET)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    // HTTP/1.0 keeps CUPS from answering in chunks.
    write!(
        stream,
        "POST / HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let start = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("no HTTP body from CUPS"))?;
    job_count(&response[start + 4..]).ok_or_else(|| std::io::Error::other("bad IPP answer"))
}

fn get_jobs_request() -> Vec<u8> {
    let mut body = vec![2, 0];
    body.extend(GET_JOBS.to_be_bytes());
    body.extend(1u32.to_be_bytes());
    body.push(OPERATION_ATTRIBUTES);
    for (tag, name, value) in [
        (CHARSET, "attributes-charset", "utf-8"),
        (NATURAL_LANGUAGE, "attributes-natural-language", "en"),
        (URI, "printer-uri", "ipp://localhost/"),
        (KEYWORD, "which-jobs", "not-completed"),
        (KEYWORD, "requested-attributes", "job-id"),
    ] {
        body.push(tag);
        body.extend((name.len() as u16).to_be_bytes());
        body.extend(name.as_bytes());
        body.extend((value.len() as u16).to_be_bytes());
        body.extend(value.as_bytes());
    }
    body.push(END_OF_ATTRIBUTES);
    body
}

fn job_count(answer: &[u8]) -> Option<usize> {
    let status = u16::from_be_bytes([*answer.get(2)?, *answer.get(3)?]);
    if status >= SUCCESS_LIMIT {
        return None;
    }
    let mut at = 8;
    let mut jobs = 0;
    loop {
        let tag = *answer.get(at)?;
        at += 1;
        if tag == END_OF_ATTRIBUTES {
            return Some(jobs);
        }
        if tag < DELIMITERS {
            continue;
        }
        let name_length = usize::from(u16::from_be_bytes([*answer.get(at)?, *answer.get(at + 1)?]));
        let name = answer.get(at + 2..at + 2 + name_length)?;
        at += 2 + name_length;
        let value_length =
            usize::from(u16::from_be_bytes([*answer.get(at)?, *answer.get(at + 1)?]));
        at += 2 + value_length;
        if name == b"job-id" {
            jobs += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attribute(tag: u8, name: &str, value: &[u8]) -> Vec<u8> {
        let mut bytes = vec![tag];
        bytes.extend((name.len() as u16).to_be_bytes());
        bytes.extend(name.as_bytes());
        bytes.extend((value.len() as u16).to_be_bytes());
        bytes.extend(value);
        bytes
    }

    fn answer(status: u16, jobs: &[u32]) -> Vec<u8> {
        let mut bytes = vec![2, 0];
        bytes.extend(status.to_be_bytes());
        bytes.extend(1u32.to_be_bytes());
        bytes.push(OPERATION_ATTRIBUTES);
        bytes.extend(attribute(CHARSET, "attributes-charset", b"utf-8"));
        for job in jobs {
            bytes.push(0x02);
            bytes.extend(attribute(0x21, "job-id", &job.to_be_bytes()));
        }
        bytes.push(END_OF_ATTRIBUTES);
        bytes
    }

    #[test]
    fn every_job_group_counts_once() {
        assert_eq!(job_count(&answer(0, &[7, 8])), Some(2));
        assert_eq!(job_count(&answer(0, &[])), Some(0));
    }

    #[test]
    fn an_error_status_or_a_cut_answer_is_not_a_count() {
        assert_eq!(job_count(&answer(0x0406, &[7])), None);
        let whole = answer(0, &[7]);
        assert_eq!(job_count(&whole[..whole.len() - 3]), None);
    }

    #[test]
    fn the_request_asks_for_unfinished_jobs_and_ends_its_attributes() {
        let request = get_jobs_request();
        assert_eq!(&request[2..4], &GET_JOBS.to_be_bytes());
        assert_eq!(request.last(), Some(&END_OF_ATTRIBUTES));
        assert!(request.windows(13).any(|window| window == b"not-completed"));
    }
}
