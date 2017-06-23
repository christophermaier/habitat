// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use error::{Error, Result};
use hab_net::server::ZMQ_CONTEXT;
use protobuf::Message;
use protocol::jobsrv::{JobLogComplete, JobLogChunk};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process;
use super::workspace::Workspace;
use zmq;

const INPROC_ADDR: &'static str = "inproc://logger";
const EOL_MARKER: &'static str = "\n";

/// ZMQ protocol frame to indicate a log line is being sent
const LOG_LINE: &'static str = "L";
/// ZMQ protocol frame to indicate a log has finished
const LOG_COMPLETE: &'static str = "C";


pub struct LogPipe {
    job_id: u64,
    sock: zmq::Socket,
    log_file: File,
}

impl LogPipe {
    pub fn new(workspace: &Workspace) -> Self {
        let sock = (**ZMQ_CONTEXT).as_mut().socket(zmq::PUSH).unwrap();
        sock.set_immediate(true).unwrap();
        sock.set_linger(5000).unwrap();
        sock.connect(INPROC_ADDR).unwrap();

        let path = workspace.root().join(format!(
            "log_pipe-{}.log",
            workspace.job.get_id()
        ));

        LogPipe {
            job_id: workspace.job.get_id(),
            sock: sock,
            log_file: File::create(path).expect("Unable to create LogPipe log file"),
        }
    }

    /// Stream log output via ZMQ back to the Job Server for
    /// aggregation and streaming to downstream clients.
    ///
    /// Contents of STDOUT are streamed before any from STDERR (if
    /// any).
    pub fn pipe(&mut self, process: &mut process::Child) -> Result<()> {
        let mut line_count = 0;

        writeln!(&self.log_file, "About to log stdout")?;
        if let Some(ref mut stdout) = process.stdout {
            let reader = BufReader::new(stdout);
            line_count = self.stream_lines(reader, line_count)?;
        }
        writeln!(&self.log_file, "Finished logging stdout")?;
        writeln!(&self.log_file, "About to log stderr")?;
        if let Some(ref mut stderr) = process.stderr {
            let reader = BufReader::new(stderr);
            // not capturing line_count output because we don't use it
            self.stream_lines(reader, line_count)?;
        }
        writeln!(&self.log_file, "Finished logging stderr")?;

        writeln!(
            &self.log_file,
            "About to tell log_forwarder that the job is complete"
        )?;
        // Signal that the log is finished
        let mut complete = JobLogComplete::new();
        complete.set_job_id(self.job_id);
        if let Err(e) = self.sock.send_str(LOG_COMPLETE, zmq::SNDMORE) {
            writeln!(
                &self.log_file,
                "ZMQ error when sending LOG_COMPLETE: {:?}",
                &e
            )?;
            return Err(Error::Zmq(e));
        }
        if let Err(e) = self.sock.send(
            complete.write_to_bytes().unwrap().as_slice(),
            0,
        )
        {
            writeln!(
                &self.log_file,
                "ZMQ error when sending JobLogComplete {:?} : {:?}",
                &complete,
                &e
            )?;
            return Err(Error::Zmq(e));
        }
        writeln!(
            &self.log_file,
            "Finished telling log_forwarder that the job is complete"
        )?;

        Ok(())
    }

    /// Send the lines of the reader out over the ZMQ socket as
    /// `JobLogChunk` messages.
    ///
    /// `line_num` is the line number to start with when generating
    /// JobLogChunk messages. This allows us to send multiple output
    /// to the same job (i.e. standard output and standard error);
    /// send the first set using `line_num` = 0, send the second using
    /// whatever value the first invocation of `stream_lines`
    /// returned, etc.
    ///
    /// (I wrestled with the type system for an alternative
    /// implementation, but it defeated me :( This seems passable in
    /// the meantime.)
    fn stream_lines<B: BufRead>(&mut self, reader: B, mut line_num: u64) -> Result<u64> {
        for line in reader.lines() {
            line_num = line_num + 1;
            let mut l: String = line.unwrap();
            l = l + EOL_MARKER;

            write!(&self.log_file, "Current line = {}", l)?;

            let mut chunk = JobLogChunk::new();
            chunk.set_job_id(self.job_id);
            chunk.set_seq(line_num);
            chunk.set_content(l.clone());

            if let Err(e) = self.sock.send_str(LOG_LINE, zmq::SNDMORE) {
                writeln!(&self.log_file, "ZMQ error when sending LOG_LINE: {:?}", &e)?;
                return Err(Error::Zmq(e));
            }
            if let Err(e) = self.sock.send(
                chunk.write_to_bytes().unwrap().as_slice(),
                0,
            )
            {
                writeln!(
                    &self.log_file,
                    "ZMQ error when sending JobLogChunk {:?} : {:?}",
                    &chunk,
                    &e
                )?;
                return Err(Error::Zmq(e));
            }
            writeln!(&self.log_file, "Finished sending ^ to log_forwarder")?;
        }

        Ok(line_num)
    }
}

impl Drop for LogPipe {
    fn drop(&mut self) {
        self.log_file.sync_all().expect("Unable to sync log file");
    }
}
