use std::convert::Infallible;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use zbus::MatchRule;
use zbus::blocking::Connection;
use zbus::message::Type;

use crate::bus::Service;

const NOTIFIER: &str = "org.cups.cupsd.Notifier";
const TIMEOUT: Duration = Duration::from_secs(2);
// IPP/2.0 over CUPS's local socket (RFC 8010, RFC 8011; https://www.cups.org/doc/spec-ipp.html).
pub const GET_JOBS: u16 = 0x000a;
pub const OPERATION_GROUP: u8 = 0x01;
pub const JOB_GROUP: u8 = 0x02;
pub const END: u8 = 0x03;
pub const INTEGER: u8 = 0x21;
pub const BOOLEAN: u8 = 0x22;
pub const ENUM: u8 = 0x23;
pub const NAME: u8 = 0x42;
pub const KEYWORD: u8 = 0x44;
pub const URI: u8 = 0x45;
const CHARSET: u8 = 0x47;
const LANGUAGE: u8 = 0x48;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Job {
    pub id: i32,
    pub name: String,
    pub state: i32,
    pub done: i32,
    pub pages: i32,
    pub printer: String,
}

#[derive(Default)]
pub struct Cups;

impl Service for Cups {
    const NAME: &'static str = NOTIFIER;
    type Snapshot = usize;
    type Command = Infallible;

    fn read(&mut self, _bus: &Connection) -> zbus::Result<usize> {
        let answer = exchange(&all_jobs_request())
            .map_err(|error| zbus::Error::Failure(error.to_string()))?;
        parse_jobs(&answer)
            .map(|jobs| jobs.len())
            .ok_or_else(|| zbus::Error::Failure("bad IPP answer".to_string()))
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

fn socket() -> PathBuf {
    std::env::var("CUPS_SERVER")
        .ok()
        .filter(|server| server.starts_with('/'))
        .map_or_else(|| PathBuf::from("/run/cups/cups.sock"), PathBuf::from)
}

pub fn attribute(buffer: &mut Vec<u8>, tag: u8, name: &str, value: &[u8]) {
    buffer.push(tag);
    buffer.extend((name.len() as u16).to_be_bytes());
    buffer.extend(name.as_bytes());
    buffer.extend((value.len() as u16).to_be_bytes());
    buffer.extend(value);
}

pub fn request(operation: u16, fill: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut buffer = vec![2, 0];
    buffer.extend(operation.to_be_bytes());
    buffer.extend(1u32.to_be_bytes());
    buffer.push(OPERATION_GROUP);
    attribute(&mut buffer, CHARSET, "attributes-charset", b"utf-8");
    attribute(&mut buffer, LANGUAGE, "attributes-natural-language", b"en");
    fill(&mut buffer);
    buffer.push(END);
    buffer
}

fn all_jobs_request() -> Vec<u8> {
    request(GET_JOBS, |buffer| {
        attribute(buffer, URI, "printer-uri", b"ipp://localhost/");
        attribute(buffer, KEYWORD, "which-jobs", b"not-completed");
        attribute(buffer, KEYWORD, "requested-attributes", b"job-id");
    })
}

pub fn parse_jobs(response: &[u8]) -> Option<Vec<Job>> {
    let status = u16::from_be_bytes([*response.get(2)?, *response.get(3)?]);
    if status >= 0x0100 {
        return None;
    }
    let mut jobs: Vec<Job> = Vec::new();
    let mut at = 8;
    let mut in_job = false;
    let mut current = String::new();
    while let Some(&tag) = response.get(at) {
        at += 1;
        if tag == END {
            break;
        }
        if tag < 0x10 {
            in_job = tag == JOB_GROUP;
            if in_job {
                jobs.push(Job::default());
            }
            continue;
        }
        let length = |at: usize| {
            Some(u16::from_be_bytes([*response.get(at)?, *response.get(at + 1)?]) as usize)
        };
        let name_length = length(at)?;
        let name = response.get(at + 2..at + 2 + name_length)?;
        at += 2 + name_length;
        let value_length = length(at)?;
        let value = response.get(at + 2..at + 2 + value_length)?;
        at += 2 + value_length;
        if !name.is_empty() {
            current = String::from_utf8_lossy(name).into_owned();
        }
        let Some(job) = jobs.last_mut().filter(|_| in_job) else {
            continue;
        };
        let number = || (tag == INTEGER || tag == ENUM) && value.len() == 4;
        let integer = i32::from_be_bytes(value.try_into().unwrap_or_default());
        let text = String::from_utf8_lossy(value).into_owned();
        match current.as_str() {
            "job-id" if number() => job.id = integer,
            "job-state" if number() => job.state = integer,
            "job-media-sheets-completed" if number() => job.done = integer,
            "job-impressions" if number() => job.pages = integer,
            "job-name" => job.name = text,
            "job-printer-uri" => {
                job.printer = text.rsplit('/').next().unwrap_or_default().to_string()
            }
            _ => {}
        }
    }
    Some(jobs)
}

pub fn exchange(body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(socket())?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply)?;
    Ok(http_body(&reply))
}

fn http_body(reply: &[u8]) -> Vec<u8> {
    let Some(split) = reply.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Vec::new();
    };
    let headers = String::from_utf8_lossy(&reply[..split]).to_ascii_lowercase();
    let body = &reply[split + 4..];
    if !headers.contains("transfer-encoding: chunked") {
        return body.to_vec();
    }
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(line) = rest.windows(2).position(|window| window == b"\r\n") {
        let size =
            usize::from_str_radix(String::from_utf8_lossy(&rest[..line]).trim(), 16).unwrap_or(0);
        if size == 0 || rest.len() < line + 2 + size {
            break;
        }
        out.extend(&rest[line + 2..line + 2 + size]);
        rest = rest.get(line + 4 + size..).unwrap_or_default();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_bar_request_asks_for_every_unfinished_job() {
        let request = all_jobs_request();
        assert_eq!(&request[..4], &[2, 0, 0x00, 0x0a]);
        assert_eq!(request.last(), Some(&END));
        assert!(request.windows(13).any(|window| window == b"not-completed"));
    }

    #[test]
    fn a_response_with_two_jobs_parses_their_progress() {
        let mut reply = vec![2, 0, 0, 0, 0, 0, 0, 1, OPERATION_GROUP];
        attribute(&mut reply, CHARSET, "attributes-charset", b"utf-8");
        for (id, name, done, pages) in [(12, "Factura.pdf", 3, 12), (13, "Foto", 0, 0)] {
            reply.push(JOB_GROUP);
            attribute(&mut reply, INTEGER, "job-id", &i32::to_be_bytes(id));
            attribute(&mut reply, NAME, "job-name", name.as_bytes());
            attribute(&mut reply, ENUM, "job-state", &5i32.to_be_bytes());
            attribute(
                &mut reply,
                INTEGER,
                "job-media-sheets-completed",
                &i32::to_be_bytes(done),
            );
            attribute(
                &mut reply,
                INTEGER,
                "job-impressions",
                &i32::to_be_bytes(pages),
            );
            attribute(
                &mut reply,
                URI,
                "job-printer-uri",
                b"ipp://localhost/printers/Oficina",
            );
        }
        reply.push(END);
        let jobs = parse_jobs(&reply).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!((jobs[0].id, jobs[0].done, jobs[0].pages), (12, 3, 12));
        assert_eq!(jobs[0].printer, "Oficina");
        assert_eq!(jobs[1].name, "Foto");
    }

    #[test]
    fn an_error_status_a_cut_answer_or_a_chunked_body_are_handled() {
        assert_eq!(parse_jobs(&[2, 0, 0x04, 0x00, 0, 0, 0, 1, END]), None);
        let mut cut = vec![2, 0, 0, 0, 0, 0, 0, 1, JOB_GROUP];
        attribute(&mut cut, INTEGER, "job-id", &7i32.to_be_bytes());
        assert_eq!(parse_jobs(&cut[..cut.len() - 3]), None);
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        assert_eq!(http_body(chunked), b"abcde");
    }
}
