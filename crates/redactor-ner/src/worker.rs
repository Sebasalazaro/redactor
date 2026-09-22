//! Running the model in a separate process.
//!
//! ONNX Runtime and the system allocator keep most of the memory a model
//! used after it is dropped (about 200 MB for the edge model). A worker
//! process gives every byte back when it exits, so an app can start one on
//! demand and end it when idle.
//!
//! The protocol is one JSON object per line: a [`Request`] on the worker's
//! stdin, a [`Response`] on its stdout.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::{Entity, Model, NerError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub text: String,
    pub labels: Vec<String>,
    pub threshold: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Worker side: loads the model, then answers requests until `input` ends.
/// A model that fails to load is reported on the first request.
pub fn serve(model_dir: &Path, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut model = Model::load(model_dir);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match (&mut model, serde_json::from_str::<Request>(&line)) {
            (Err(e), _) => error(e.to_string()),
            (_, Err(e)) => error(format!("bad request: {e}")),
            (Ok(model), Ok(request)) => {
                let labels: Vec<&str> = request.labels.iter().map(String::as_str).collect();
                match model.detect(&request.text, &labels, request.threshold) {
                    Ok(entities) => Response {
                        entities,
                        error: None,
                    },
                    Err(e) => error(e.to_string()),
                }
            }
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn error(message: String) -> Response {
    Response {
        entities: Vec::new(),
        error: Some(message),
    }
}

/// App side: a running worker. Dropping it ends the process.
pub struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Client {
    /// Starts `command` (which must end up calling [`serve`]) as a worker.
    pub fn spawn(mut command: Command) -> std::io::Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn detect(
        &mut self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<Entity>, NerError> {
        let request = Request {
            text: text.to_string(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            threshold,
        };
        let io = |e: std::io::Error| NerError::Worker(e.to_string());
        let mut line =
            serde_json::to_string(&request).map_err(|e| NerError::Worker(e.to_string()))?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).map_err(io)?;
        self.stdin.flush().map_err(io)?;

        let mut reply = String::new();
        if self.stdout.read_line(&mut reply).map_err(io)? == 0 {
            return Err(NerError::Worker("the worker exited".into()));
        }
        let response: Response =
            serde_json::from_str(&reply).map_err(|e| NerError::Worker(e.to_string()))?;
        match response.error {
            Some(e) => Err(NerError::Worker(e)),
            None => Ok(response.entities),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_missing_model_per_request() {
        let input = b"{\"text\": \"hi\", \"labels\": [\"name\"], \"threshold\": 0.3}\n\n";
        let mut output = Vec::new();
        serve(Path::new("/nonexistent"), &input[..], &mut output).unwrap();
        let response: Response = serde_json::from_slice(&output).unwrap();
        assert!(response.error.unwrap().contains("not found"));
    }

    #[test]
    fn reports_bad_requests() {
        let mut output = Vec::new();
        serve(Path::new("/nonexistent"), &b"not json\n"[..], &mut output).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("error"));
    }
}
